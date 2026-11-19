//! The Whisper model: downloaded once from the official ggml files on Hugging
//! Face into the app's data folder, checked against its SHA-256, and resumed
//! where it stopped when the connection drops.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Where whisper.cpp publishes its models.
pub const DEFAULT_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// One model file and what it must hash to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec<'a> {
    pub name: &'a str,
    pub file: &'a str,
    /// Bytes.
    pub size: u64,
    /// Lowercase hex.
    pub sha256: &'a str,
}

/// Multilingual and small enough to download on first use: about 148 MB.
pub const BASE: ModelSpec<'static> = ModelSpec {
    name: "Whisper base",
    file: "ggml-base.bin",
    size: 147_951_465,
    sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
};

/// The smallest multilingual model, about 78 MB, for tests.
pub const TINY: ModelSpec<'static> = ModelSpec {
    name: "Whisper tiny",
    file: "ggml-tiny.bin",
    size: 77_691_713,
    sha256: "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
};

impl ModelSpec<'_> {
    pub fn url(&self, base_url: &str) -> String {
        format!("{}/{}", base_url.trim_end_matches('/'), self.file)
    }

    /// The model in `dir`, when a complete download is there. A finished
    /// download was checked when it finished, so only its size is checked here.
    pub fn installed(&self, dir: &Path) -> Option<PathBuf> {
        let path = dir.join(self.file);
        let size = fs::metadata(&path).ok()?.len();
        (size == self.size).then_some(path)
    }

    fn partial(&self, dir: &Path) -> PathBuf {
        dir.join(format!("{}.part", self.file))
    }

    /// Bytes of an unfinished download in `dir`, to resume from.
    pub fn partial_size(&self, dir: &Path) -> u64 {
        fs::metadata(self.partial(dir)).map_or(0, |meta| meta.len())
    }

    /// Deletes the model and any unfinished download.
    pub fn remove(&self, dir: &Path) -> io::Result<()> {
        for path in [dir.join(self.file), self.partial(dir)] {
            match fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DownloadError {
    #[error("no internet connection")]
    Offline,
    #[error("the model server answered HTTP {0}")]
    Http(u16),
    #[error("the downloaded model is damaged (wrong checksum)")]
    ChecksumMismatch,
    #[error("the model could not be saved: {0}")]
    Storage(String),
    #[error("the download was cancelled")]
    Cancelled,
    #[error("the download failed: {0}")]
    Network(String),
}

impl DownloadError {
    /// Worth trying again at once, picking up where it stopped.
    fn is_transient(&self) -> bool {
        match self {
            DownloadError::Offline | DownloadError::Network(_) => true,
            DownloadError::Http(status) => *status >= 500 || *status == 408 || *status == 429,
            _ => false,
        }
    }
}

/// How the download goes on after a dropped connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retry {
    pub attempts: u32,
    /// The pause before the first retry; it doubles for each one after.
    pub pause: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            attempts: 4,
            pause: Duration::from_secs(2),
        }
    }
}

/// An HTTP client for the download: no limit on the whole transfer, which
/// takes minutes on a slow line, but on connecting and on every read.
pub fn http_client(user_agent: &str) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

/// Bytes downloaded and expected, for a progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub received: u64,
    pub total: u64,
}

/// Downloads `spec` from `url` into `dir` and returns the model's path. An
/// unfinished earlier download is resumed with an HTTP range request; a file
/// that does not match the checksum is deleted, so the next try starts over.
pub async fn download(
    spec: &ModelSpec<'_>,
    url: &str,
    dir: &Path,
    http: &reqwest::Client,
    retry: Retry,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf, DownloadError> {
    if let Some(path) = spec.installed(dir) {
        return Ok(path);
    }
    fs::create_dir_all(dir).map_err(storage)?;
    let partial = spec.partial(dir);
    let mut failures = 0;
    loop {
        match fetch(spec, url, &partial, http, cancel, &mut progress).await {
            Ok(()) => break,
            Err(error) if error.is_transient() && failures < retry.attempts => {
                let pause = retry.pause.saturating_mul(1 << failures.min(6));
                failures += 1;
                tokio::time::sleep(pause).await;
                if cancel.load(Ordering::SeqCst) {
                    return Err(DownloadError::Cancelled);
                }
            }
            Err(error) => return Err(error),
        }
    }
    let path = partial.clone();
    let digest = tokio::task::spawn_blocking(move || sha256_of(&path))
        .await
        .map_err(|error| storage(io::Error::other(error)))?
        .map_err(storage)?;
    if !digest.eq_ignore_ascii_case(spec.sha256) {
        let _ = fs::remove_file(&partial);
        return Err(DownloadError::ChecksumMismatch);
    }
    let path = dir.join(spec.file);
    fs::rename(&partial, &path).map_err(storage)?;
    Ok(path)
}

/// Brings the partial file to its full size, resuming from what it has.
async fn fetch(
    spec: &ModelSpec<'_>,
    url: &str,
    partial: &Path,
    http: &reqwest::Client,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Result<(), DownloadError> {
    let mut have = fs::metadata(partial).map_or(0, |meta| meta.len());
    if have > spec.size {
        File::create(partial).map_err(storage)?;
        have = 0;
    }
    if have == spec.size {
        return Ok(());
    }
    let mut request = http.get(url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let mut response = request.send().await.map_err(network)?;
    let status = response.status().as_u16();
    let mut file = match status {
        // The server resumes where the file stops.
        206 if resumes_at(&response, have) => OpenOptions::new()
            .append(true)
            .open(partial)
            .map_err(storage)?,
        // The whole file again: the server ignored the range, or there was none.
        200 => {
            have = 0;
            File::create(partial).map_err(storage)?
        }
        // The range lies past the end: what is there is wrong, start over.
        206 | 416 => {
            File::create(partial).map_err(storage)?;
            return Err(DownloadError::Network("the server could not resume".into()));
        }
        other => return Err(DownloadError::Http(other)),
    };
    progress(Progress {
        received: have,
        total: spec.size,
    });
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if cancel.load(Ordering::SeqCst) {
            return Err(DownloadError::Cancelled);
        }
        if have + chunk.len() as u64 > spec.size {
            drop(file);
            File::create(partial).map_err(storage)?;
            return Err(DownloadError::ChecksumMismatch);
        }
        file.write_all(&chunk).map_err(storage)?;
        have += chunk.len() as u64;
        progress(Progress {
            received: have,
            total: spec.size,
        });
    }
    file.flush().map_err(storage)?;
    if have < spec.size {
        return Err(DownloadError::Network("the connection closed early".into()));
    }
    Ok(())
}

fn resumes_at(response: &reqwest::Response, offset: u64) -> bool {
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("bytes "))
        .and_then(|range| range.split('-').next())
        .and_then(|start| start.trim().parse::<u64>().ok())
        == Some(offset)
}

/// The SHA-256 of a file, in lowercase hex.
pub fn sha256_of(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn storage(error: io::Error) -> DownloadError {
    DownloadError::Storage(error.to_string())
}

fn network(error: reqwest::Error) -> DownloadError {
    if error.is_connect() {
        DownloadError::Offline
    } else if error.is_timeout() {
        DownloadError::Network("timed out".into())
    } else {
        DownloadError::Network(error.to_string())
    }
}
