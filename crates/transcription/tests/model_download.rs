//! The model download against a local pretend server: checksums, resuming,
//! retrying and cancelling.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Duration;

use transcription::model::{self, DownloadError, ModelSpec, Progress, Retry};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const QUICK: Retry = Retry {
    attempts: 2,
    pause: Duration::ZERO,
};

fn content() -> Vec<u8> {
    (0..100u8).map(|byte| byte.wrapping_mul(7)).collect()
}

/// A spec for [`content`], with its real checksum.
fn spec(file: &'static str) -> ModelSpec<'static> {
    let directory = scratch("hash");
    let probe = directory.join("probe");
    std::fs::write(&probe, content()).expect("written");
    let sha256 = model::sha256_of(&probe).expect("hashed");
    ModelSpec {
        name: "Test model",
        file,
        size: 100,
        sha256: Box::leak(sha256.into_boxed_str()),
    }
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "singline-model-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("created");
    directory
}

async fn run(
    spec: &ModelSpec<'_>,
    server: &MockServer,
    directory: &std::path::Path,
) -> (Result<PathBuf, DownloadError>, Vec<Progress>) {
    let seen = Mutex::new(Vec::new());
    let result = model::download(
        spec,
        &spec.url(&server.uri()),
        directory,
        &reqwest::Client::new(),
        QUICK,
        &AtomicBool::new(false),
        |progress| seen.lock().expect("progress").push(progress),
    )
    .await;
    (result, seen.into_inner().expect("progress"))
}

#[tokio::test]
async fn downloads_and_checks_the_model() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/fresh.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content()))
        .expect(1)
        .mount(&server)
        .await;
    let directory = scratch("fresh");
    let spec = spec("fresh.bin");
    let (result, progress) = run(&spec, &server, &directory).await;
    let installed = result.expect("downloads");
    assert_eq!(std::fs::read(&installed).expect("read"), content());
    assert_eq!(spec.installed(&directory), Some(installed));
    assert_eq!(spec.partial_size(&directory), 0);
    assert_eq!(
        progress.last(),
        Some(&Progress {
            received: 100,
            total: 100
        })
    );
    // Once installed, nothing is downloaded again.
    assert!(run(&spec, &server, &directory).await.0.is_ok());
    spec.remove(&directory).expect("removed");
    assert_eq!(spec.installed(&directory), None);
}

#[tokio::test]
async fn a_damaged_download_is_thrown_away() {
    let server = MockServer::start().await;
    let mut damaged = content();
    damaged[50] ^= 0xff;
    Mock::given(path("/damaged.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(damaged))
        .mount(&server)
        .await;
    let directory = scratch("damaged");
    let spec = spec("damaged.bin");
    let (result, _) = run(&spec, &server, &directory).await;
    assert_eq!(result, Err(DownloadError::ChecksumMismatch));
    assert_eq!(spec.installed(&directory), None);
    assert_eq!(spec.partial_size(&directory), 0, "the next try starts over");
}

#[tokio::test]
async fn resumes_where_the_last_download_stopped() {
    let server = MockServer::start().await;
    Mock::given(path("/resume.bin"))
        .and(header("range", "bytes=40-"))
        .respond_with(
            ResponseTemplate::new(206)
                .insert_header("content-range", "bytes 40-99/100")
                .set_body_bytes(content()[40..].to_vec()),
        )
        .expect(1)
        .mount(&server)
        .await;
    let directory = scratch("resume");
    let spec = spec("resume.bin");
    std::fs::write(directory.join("resume.bin.part"), &content()[..40]).expect("written");
    assert_eq!(spec.partial_size(&directory), 40);
    let (result, progress) = run(&spec, &server, &directory).await;
    assert_eq!(
        std::fs::read(result.expect("resumes")).expect("read"),
        content()
    );
    assert_eq!(progress.first().map(|p| p.received), Some(40));
}

#[tokio::test]
async fn starts_over_when_the_server_cannot_resume() {
    let server = MockServer::start().await;
    Mock::given(path("/restart.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content()))
        .mount(&server)
        .await;
    let directory = scratch("restart");
    let spec = spec("restart.bin");
    std::fs::write(directory.join("restart.bin.part"), [1u8; 40]).expect("written");
    let (result, _) = run(&spec, &server, &directory).await;
    assert_eq!(
        std::fs::read(result.expect("downloads")).expect("read"),
        content()
    );
}

#[tokio::test]
async fn retries_after_a_server_error_but_not_after_a_missing_file() {
    let server = MockServer::start().await;
    Mock::given(path("/flaky.bin"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path("/flaky.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content()))
        .mount(&server)
        .await;
    let directory = scratch("flaky");
    let flaky = spec("flaky.bin");
    assert!(run(&flaky, &server, &directory).await.0.is_ok());

    let missing = spec("missing.bin");
    let (result, _) = run(&missing, &server, &directory).await;
    assert_eq!(result, Err(DownloadError::Http(404)));
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/missing.bin")
            .count(),
        1,
        "a missing file is not asked for again"
    );
}

#[tokio::test]
async fn cancelling_keeps_what_arrived_for_later() {
    let server = MockServer::start().await;
    Mock::given(path("/cancel.bin"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(content()))
        .mount(&server)
        .await;
    let directory = scratch("cancel");
    let spec = spec("cancel.bin");
    let result = model::download(
        &spec,
        &spec.url(&server.uri()),
        &directory,
        &reqwest::Client::new(),
        QUICK,
        &AtomicBool::new(true),
        |_| {},
    )
    .await;
    assert_eq!(result, Err(DownloadError::Cancelled));
    assert_eq!(spec.installed(&directory), None);
}
