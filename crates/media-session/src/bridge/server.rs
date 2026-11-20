//! Accepts connections on 127.0.0.1 and answers the extension.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use super::http::{read_request, ReadError, Request, Response};
use super::pairing::PairError;
use super::protocol::{sample_instant, PairRequest, UpdateRequest, MAX_CLOSED, MAX_TABS};
use super::{unix_ms, Shared, Tab};

/// Connections handled at the same time; more are closed at once.
const MAX_CONNECTIONS: usize = 16;
/// How long a connection may take to send its request or read the answer.
const IO_TIMEOUT: Duration = Duration::from_secs(3);
/// When no port was free, how long to wait before trying again.
const RETRY: Duration = Duration::from_secs(15);
const PROTOCOL: u32 = 1;

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn spawn(shared: Arc<Mutex<Shared>>, ports: Vec<u16>) {
    let started = thread::Builder::new()
        .name("browser-bridge".into())
        .spawn(move || loop {
            match bind(&ports) {
                Ok((listener, port)) => {
                    {
                        let mut state = lock(&shared);
                        state.port = Some(port);
                        state.error = None;
                    }
                    serve(&listener, &shared, port);
                    lock(&shared).port = None;
                }
                Err(error) => lock(&shared).error = Some(error),
            }
            thread::sleep(RETRY);
        });
    if let Err(error) = started {
        log::error!("could not start the browser bridge: {error}");
    }
}

/// Listens on the first free port, on the loopback interface only.
fn bind(ports: &[u16]) -> Result<(TcpListener, u16), String> {
    let mut last = String::from("no port to listen on");
    for &port in ports {
        match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => {
                let port = listener
                    .local_addr()
                    .map_err(|error| error.to_string())?
                    .port();
                return Ok((listener, port));
            }
            Err(error) => last = format!("port {port}: {error}"),
        }
    }
    Err(last)
}

fn serve(listener: &TcpListener, shared: &Arc<Mutex<Shared>>, port: u16) {
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            // Out of file handles, for example; do not spin.
            thread::sleep(Duration::from_millis(50));
            continue;
        };
        let local = stream
            .peer_addr()
            .is_ok_and(|address| address.ip().is_loopback());
        if !local || active.load(Ordering::SeqCst) >= MAX_CONNECTIONS {
            continue;
        }
        active.fetch_add(1, Ordering::SeqCst);
        let shared = Arc::clone(shared);
        let counter = Arc::clone(&active);
        let spawned = thread::Builder::new()
            .name("browser-bridge-request".into())
            .spawn(move || {
                handle(stream, &shared, port);
                counter.fetch_sub(1, Ordering::SeqCst);
            });
        if spawned.is_err() {
            active.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

fn handle(mut stream: TcpStream, shared: &Mutex<Shared>, port: u16) {
    let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let response = match read_request(&mut stream) {
        Ok(request) => respond(shared, &request, port, Instant::now(), unix_ms()),
        Err(ReadError::Closed) => return,
        Err(ReadError::TooLarge) => Response::error(413, "tooLarge"),
        Err(ReadError::LengthRequired) => Response::error(411, "lengthRequired"),
        Err(ReadError::Malformed) => Response::error(400, "badRequest"),
    };
    let _ = response.write_to(&mut stream);
}

/// Only the extension's own pages may call; web pages may not.
fn extension_origin(origin: &str) -> bool {
    let origin = origin.to_ascii_lowercase();
    [
        "chrome-extension://",
        "moz-extension://",
        "safari-web-extension://",
    ]
    .iter()
    .any(|scheme| origin.starts_with(scheme) && origin.len() > scheme.len())
}

/// The Host header names this server, which defeats DNS rebinding: a web
/// page that makes its own name point at 127.0.0.1 still sends its name.
fn our_host(host: Option<&str>, port: u16) -> bool {
    host.is_some_and(|host| {
        let host = host.to_ascii_lowercase();
        host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
    })
}

/// Answers one request. Pure apart from the shared state, so it is tested
/// without sockets.
pub(super) fn respond(
    shared: &Mutex<Shared>,
    request: &Request,
    port: u16,
    now: Instant,
    now_unix_ms: f64,
) -> Response {
    let origin = request.header("origin");
    let allowed = origin.filter(|origin| extension_origin(origin));
    if (origin.is_some() && allowed.is_none()) || !our_host(request.header("host"), port) {
        return Response::error(403, "forbidden");
    }
    let response = route(shared, request, now, now_unix_ms);
    match allowed {
        Some(origin) => response
            .with_header("Access-Control-Allow-Origin", origin)
            .with_header("Vary", "Origin"),
        None => response,
    }
}

fn route(shared: &Mutex<Shared>, request: &Request, now: Instant, now_unix_ms: f64) -> Response {
    let path = request.path.split('?').next().unwrap_or_default();
    let known = matches!(path, "/v1/hello" | "/v1/pair" | "/v1/update" | "/v1/unpair");
    if request.method == "OPTIONS" && known {
        return preflight(request);
    }
    let json_body = request
        .header("content-type")
        .is_some_and(|value| value.to_ascii_lowercase().starts_with("application/json"));
    match (request.method.as_str(), path) {
        ("GET", "/v1/hello") => {
            let version = lock(shared).version.clone();
            Response::json(
                200,
                &json!({ "app": "singline", "protocol": PROTOCOL, "version": version }),
            )
        }
        ("POST", _) if known && !json_body => Response::error(415, "notJson"),
        ("POST", "/v1/pair") => pair(shared, request, now, now_unix_ms),
        ("POST", "/v1/update") => update(shared, request, now, now_unix_ms),
        ("POST", "/v1/unpair") => {
            let mut state = lock(shared);
            let Some(client) = authenticate(&mut state, request, now) else {
                return Response::error(401, "unauthorized");
            };
            state.forget(&client);
            state.clients.remove(&client);
            Response::json(200, &json!({}))
        }
        _ if known => Response::error(405, "methodNotAllowed"),
        _ => Response::error(404, "notFound"),
    }
}

fn preflight(request: &Request) -> Response {
    let response = Response::empty(204)
        .with_header("Access-Control-Allow-Methods", "GET, POST")
        .with_header(
            "Access-Control-Allow-Headers",
            "Authorization, Content-Type",
        )
        .with_header("Access-Control-Max-Age", "600");
    if request.header("access-control-request-private-network") == Some("true") {
        response.with_header("Access-Control-Allow-Private-Network", "true")
    } else {
        response
    }
}

/// The id of the paired browser whose token the request carries.
fn authenticate(state: &mut Shared, request: &Request, now: Instant) -> Option<String> {
    let token = request
        .header("authorization")?
        .strip_prefix("Bearer ")?
        .trim();
    state
        .clients
        .authenticate(token, now)
        .map(|client| client.id)
}

fn pair(shared: &Mutex<Shared>, request: &Request, now: Instant, now_unix_ms: f64) -> Response {
    let Ok(body) = serde_json::from_slice::<PairRequest>(&request.body) else {
        return Response::error(400, "badRequest");
    };
    let mut state = lock(shared);
    match state.pairing.try_code(&body.code, now) {
        Ok(()) => {
            let paired_at = (now_unix_ms / 1_000.0).max(0.0) as u64;
            match state.clients.add(&body.browser, paired_at) {
                Some((client, token)) => {
                    Response::json(200, &json!({ "token": token, "clientId": client.id }))
                }
                None => Response::error(503, "unavailable"),
            }
        }
        Err(PairError::WrongCode) => Response::error(403, "wrongCode"),
        Err(PairError::Closed) => Response::error(403, "pairingClosed"),
    }
}

fn update(shared: &Mutex<Shared>, request: &Request, now: Instant, now_unix_ms: f64) -> Response {
    let mut state = lock(shared);
    let Some(client) = authenticate(&mut state, request, now) else {
        return Response::error(401, "unauthorized");
    };
    let Ok(body) = serde_json::from_slice::<UpdateRequest>(&request.body) else {
        return Response::error(400, "badRequest");
    };
    let browser = state
        .clients
        .list()
        .into_iter()
        .find(|(paired, _, _)| paired.id == client)
        .map(|(paired, _, _)| paired.name)
        .unwrap_or_default();
    for tab in body.tabs.into_iter().take(MAX_TABS) {
        let update = tab.sanitized();
        let sampled_at = sample_instant(update.sampled_at, now, now_unix_ms);
        state.tabs.insert(
            (client.clone(), update.tab_id),
            Tab {
                browser: browser.clone(),
                update,
                sampled_at,
                received_at: now,
            },
        );
    }
    for tab_id in body.closed.into_iter().take(MAX_CLOSED) {
        let key = (client.clone(), tab_id);
        state.tabs.remove(&key);
        state.commands.remove(&key);
    }
    state.expire(now);
    let mine: Vec<_> = state
        .commands
        .keys()
        .filter(|(owner, _)| *owner == client)
        .cloned()
        .collect();
    let commands: Vec<serde_json::Value> = mine
        .into_iter()
        .flat_map(|key| {
            let queue = state.commands.remove(&key).unwrap_or_default();
            queue
                .into_iter()
                .map(move |(command, _)| json!({ "tabId": key.1, "command": command }))
        })
        .collect();
    Response::json(200, &json!({ "commands": commands }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Backend;
    use crate::bridge::{Bridge, BridgeBackend, BridgeOptions};
    use crate::model::{Command, Reading, Service};
    use std::io::{Read, Write};

    const PORT: u16 = 17464;
    const NOW_MS: f64 = 1_800_000_000_000.0;

    fn state() -> Arc<Mutex<Shared>> {
        Arc::new(Mutex::new(Shared::new(&BridgeOptions::new("1.2.3"))))
    }

    fn request(method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Request {
        let mut all = vec![("host".to_owned(), format!("127.0.0.1:{PORT}"))];
        for (name, value) in headers {
            all.retain(|(existing, _)| existing != name);
            all.push(((*name).to_owned(), (*value).to_owned()));
        }
        Request {
            method: method.into(),
            path: path.into(),
            headers: all,
            body: body.as_bytes().to_vec(),
        }
    }

    fn post(path: &str, token: Option<&str>, body: &str) -> Request {
        let bearer = token.map(|token| format!("Bearer {token}"));
        let mut headers = vec![("content-type", "application/json")];
        if let Some(bearer) = &bearer {
            headers.push(("authorization", bearer.as_str()));
        }
        request("POST", path, &headers, body)
    }

    fn answer(response: &Response) -> serde_json::Value {
        serde_json::from_str(&response.body).unwrap_or(serde_json::Value::Null)
    }

    /// Pairs a browser called Chrome and returns its token.
    fn paired(shared: &Mutex<Shared>) -> String {
        let code = lock(shared).pairing.show(Instant::now()).expect("a code");
        let body = json!({ "code": code, "browser": "Chrome" }).to_string();
        let response = respond(
            shared,
            &post("/v1/pair", None, &body),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(response.status, 200, "{}", response.body);
        answer(&response)["token"]
            .as_str()
            .expect("a token")
            .to_owned()
    }

    fn update_body(tab_id: u64, title: &str, extra: serde_json::Value) -> String {
        let mut tab = json!({
            "tabId": tab_id,
            "url": "https://music.youtube.com/watch?v=1",
            "title": title,
            "artist": "Example Artist",
            "duration": 200.0,
            "currentTime": 30.0,
            "playbackRate": 1.0,
            "paused": false,
            "sampledAt": NOW_MS - 100.0,
            "controls": { "playPause": true, "next": true, "previous": false, "seek": true }
        });
        if let (Some(tab), Some(extra)) = (tab.as_object_mut(), extra.as_object()) {
            tab.extend(extra.clone());
        }
        json!({ "tabs": [tab] }).to_string()
    }

    fn backend(shared: &Arc<Mutex<Shared>>) -> BridgeBackend {
        BridgeBackend {
            shared: Arc::clone(shared),
        }
    }

    #[test]
    fn says_hello_to_the_extension_only() {
        let shared = state();
        let hello = respond(
            &shared,
            &request("GET", "/v1/hello", &[], ""),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(hello.status, 200);
        assert_eq!(
            answer(&hello),
            json!({ "app": "singline", "protocol": 1, "version": "1.2.3" })
        );
        let page = request(
            "GET",
            "/v1/hello",
            &[("origin", "https://evil.example")],
            "",
        );
        assert_eq!(
            respond(&shared, &page, PORT, Instant::now(), NOW_MS).status,
            403
        );
        let null = request("GET", "/v1/hello", &[("origin", "null")], "");
        assert_eq!(
            respond(&shared, &null, PORT, Instant::now(), NOW_MS).status,
            403
        );

        let extension = request(
            "GET",
            "/v1/hello",
            &[("origin", "chrome-extension://abcdefghijklmnop")],
            "",
        );
        let allowed = respond(&shared, &extension, PORT, Instant::now(), NOW_MS);
        assert_eq!(allowed.status, 200);
        assert!(allowed.headers.contains(&(
            "Access-Control-Allow-Origin",
            "chrome-extension://abcdefghijklmnop".to_owned()
        )));
    }

    #[test]
    fn refuses_other_host_names() {
        let shared = state();
        for host in ["evil.example:17464", "127.0.0.1:9999", "127.0.0.1"] {
            let rebound = request("GET", "/v1/hello", &[("host", host)], "");
            assert_eq!(
                respond(&shared, &rebound, PORT, Instant::now(), NOW_MS).status,
                403,
                "{host}"
            );
        }
        let mut missing = request("GET", "/v1/hello", &[], "");
        missing.headers.clear();
        assert_eq!(
            respond(&shared, &missing, PORT, Instant::now(), NOW_MS).status,
            403
        );
        let localhost = request("GET", "/v1/hello", &[("host", "LOCALHOST:17464")], "");
        assert_eq!(
            respond(&shared, &localhost, PORT, Instant::now(), NOW_MS).status,
            200
        );
    }

    #[test]
    fn pairs_only_with_the_shown_code() {
        let shared = state();
        let body = json!({ "code": "123456", "browser": "Chrome" }).to_string();
        let closed = respond(
            &shared,
            &post("/v1/pair", None, &body),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(
            (closed.status, answer(&closed)["error"].clone()),
            (403, json!("pairingClosed"))
        );

        let code = lock(&shared).pairing.show(Instant::now()).expect("a code");
        let wrong = if code == "123456" { "654321" } else { "123456" };
        let body = json!({ "code": wrong }).to_string();
        let refused = respond(
            &shared,
            &post("/v1/pair", None, &body),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(answer(&refused)["error"], json!("wrongCode"));

        let not_json = request("POST", "/v1/pair", &[("content-type", "text/plain")], &body);
        assert_eq!(
            respond(&shared, &not_json, PORT, Instant::now(), NOW_MS).status,
            415
        );
        let garbage = respond(
            &shared,
            &post("/v1/pair", None, "{"),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(garbage.status, 400);

        let token = paired(&shared);
        assert_eq!(token.len(), 64);
        let statuses = lock(&shared).clients.list();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].0.name, "Chrome");
    }

    #[test]
    fn turns_updates_into_readings_and_hands_back_commands() {
        let shared = state();
        let token = paired(&shared);
        let unauthorized = respond(
            &shared,
            &post(
                "/v1/update",
                Some(&"0".repeat(64)),
                &update_body(5, "Song", json!({})),
            ),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(unauthorized.status, 401);
        assert!(
            lock(&shared).tabs.is_empty(),
            "nothing is taken from strangers"
        );

        let accepted = respond(
            &shared,
            &post(
                "/v1/update",
                Some(&token),
                &update_body(5, "Song", json!({})),
            ),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(
            (accepted.status, answer(&accepted)),
            (200, json!({ "commands": [] }))
        );

        let mut backend = backend(&shared);
        let readings = backend.poll();
        let [Reading::Snapshot(tab)] = readings.as_slice() else {
            panic!("expected one tab, got {readings:?}");
        };
        assert_eq!(tab.player.service, Service::YouTubeMusic);
        assert_eq!(tab.player.name, "Chrome");
        assert!(tab.player.id.as_str().starts_with("bridge:"));
        let position = tab.position_at(Instant::now()).expect("a position");
        assert!(
            (30.0..30.5).contains(&position),
            "moved on since sampling: {position}"
        );

        backend
            .send(&tab.player.id, Command::SeekTo { position: 42.0 })
            .expect("seek is accepted");
        assert_eq!(
            backend.send(&tab.player.id, Command::Previous),
            Err(crate::backend::CommandError::Unsupported)
        );
        assert_eq!(
            backend.send(
                &crate::model::PlayerId::new("bridge:other:5"),
                Command::Next
            ),
            Err(crate::backend::CommandError::PlayerGone)
        );
        let with_command = respond(
            &shared,
            &post(
                "/v1/update",
                Some(&token),
                &update_body(5, "Song", json!({})),
            ),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(
            answer(&with_command),
            json!({ "commands": [{ "tabId": 5, "command": { "kind": "seekTo", "position": 42.0 } }] })
        );

        let closed = json!({ "closed": [5] }).to_string();
        respond(
            &shared,
            &post("/v1/update", Some(&token), &closed),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert!(backend.poll().is_empty(), "a closed tab is gone");
    }

    #[test]
    fn silent_tabs_and_forgotten_browsers_disappear() {
        let shared = state();
        let token = paired(&shared);
        let long_ago = Instant::now() - Duration::from_secs(30);
        respond(
            &shared,
            &post(
                "/v1/update",
                Some(&token),
                &update_body(1, "Playing", json!({})),
            ),
            PORT,
            long_ago,
            NOW_MS,
        );
        respond(
            &shared,
            &post(
                "/v1/update",
                Some(&token),
                &update_body(2, "Paused", json!({ "paused": true })),
            ),
            PORT,
            long_ago,
            NOW_MS,
        );
        let titles: Vec<String> = backend(&shared)
            .poll()
            .iter()
            .filter_map(|reading| match reading {
                Reading::Snapshot(snapshot) => snapshot.track.as_ref().map(|t| t.title.clone()),
                Reading::Unavailable { .. } => None,
            })
            .collect();
        assert_eq!(titles, ["Paused"], "a playing tab that went quiet is gone");

        let bridge = Bridge {
            shared: Arc::clone(&shared),
        };
        let id = bridge.status().browsers[0].id.clone();
        assert!(bridge.forget(&id));
        assert!(backend(&shared).poll().is_empty());
        let after = respond(
            &shared,
            &post(
                "/v1/update",
                Some(&token),
                &update_body(3, "Song", json!({})),
            ),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(after.status, 401, "the token stops working");
    }

    #[test]
    fn the_extension_can_unpair_itself() {
        let shared = state();
        let token = paired(&shared);
        let response = respond(
            &shared,
            &post("/v1/unpair", Some(&token), "{}"),
            PORT,
            Instant::now(),
            NOW_MS,
        );
        assert_eq!(response.status, 200);
        assert!(lock(&shared).clients.list().is_empty());
    }

    #[test]
    fn answers_preflights_and_unknown_paths() {
        let shared = state();
        let preflight = request(
            "OPTIONS",
            "/v1/update",
            &[
                ("origin", "moz-extension://4f0e3c1d"),
                ("access-control-request-private-network", "true"),
            ],
            "",
        );
        let response = respond(&shared, &preflight, PORT, Instant::now(), NOW_MS);
        assert_eq!(response.status, 204);
        assert!(response
            .headers
            .contains(&("Access-Control-Allow-Private-Network", "true".to_owned())));
        assert_eq!(
            respond(
                &shared,
                &request("GET", "/v1/update", &[], ""),
                PORT,
                Instant::now(),
                NOW_MS
            )
            .status,
            405
        );
        assert_eq!(
            respond(
                &shared,
                &request("GET", "/", &[], ""),
                PORT,
                Instant::now(),
                NOW_MS
            )
            .status,
            404
        );
    }

    /// The whole way over a socket, as the extension uses it.
    #[test]
    fn serves_on_the_loopback_interface() {
        let bridge = Bridge::start(BridgeOptions {
            ports: vec![0],
            ..BridgeOptions::new("1.2.3")
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        let port = loop {
            if let Some(port) = bridge.status().port {
                break port;
            }
            assert!(Instant::now() < deadline, "the bridge did not start");
            thread::sleep(Duration::from_millis(10));
        };
        let exchange = |raw: String| {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
            stream.write_all(raw.as_bytes()).expect("sent");
            let mut reply = String::new();
            stream.read_to_string(&mut reply).expect("an answer");
            reply
        };
        let hello = exchange(format!(
            "GET /v1/hello HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
        ));
        assert!(hello.starts_with("HTTP/1.1 200 OK"), "{hello}");
        assert!(hello.ends_with(r#"{"app":"singline","protocol":1,"version":"1.2.3"}"#));

        let code = bridge.pairing_code().expect("a code");
        let body = json!({ "code": code, "browser": "Firefox" }).to_string();
        let paired = exchange(format!(
            "POST /v1/pair HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ));
        assert!(paired.starts_with("HTTP/1.1 200 OK"), "{paired}");

        let too_large = exchange(format!(
            "POST /v1/update HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: 999999\r\n\r\n"
        ));
        assert!(too_large.starts_with("HTTP/1.1 413"), "{too_large}");
        let browsers = bridge.status().browsers;
        assert_eq!(
            (browsers[0].name.as_str(), browsers[0].tabs),
            ("Firefox", 0)
        );
    }
}
