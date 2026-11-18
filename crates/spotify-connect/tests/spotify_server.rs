//! The login, the tokens and the Web API calls against a pretend Spotify.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;
use spotify_connect::credentials::UnavailableCredentials;
use spotify_connect::{
    pkce, CallbackPages, Config, CredentialStore, LoginError, MemoryCredentials, Persistence,
    SpotifyClient, SpotifyError, TimeRange, TopKind, LIKED_SONGS,
};
use wiremock::matchers::{body_json, body_string_contains, header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const CLIENT_ID: &str = "0123456789abcdef0123456789abcdef";

fn config(server: &MockServer) -> Config {
    Config {
        accounts_url: server.uri(),
        api_url: format!("{}/v1", server.uri()),
        callback_port: 0,
        ..Config::new(CLIENT_ID, "Singline/test")
    }
}

fn client(server: &MockServer, store: Arc<dyn CredentialStore>) -> SpotifyClient {
    SpotifyClient::new(config(server), store)
}

fn token(access: &str, refresh: Option<&str>) -> serde_json::Value {
    let mut token = json!({
        "access_token": access, "token_type": "Bearer", "expires_in": 3600,
        "scope": pkce::SCOPES.join(" ")
    });
    if let Some(refresh) = refresh {
        token["refresh_token"] = json!(refresh);
    }
    token
}

async fn token_endpoint(server: &MockServer, grant: &str, answer: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains(format!("grant_type={grant}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

async fn me_endpoint(server: &MockServer, access: &str) {
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .and(header("authorization", format!("Bearer {access}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "julian", "display_name": "Julian"})),
        )
        .mount(server)
        .await;
}

/// A client that has a login and a valid access token.
async fn connected(server: &MockServer) -> SpotifyClient {
    token_endpoint(server, "refresh_token", token("access-1", None)).await;
    client(
        server,
        Arc::new(MemoryCredentials::with_secret("refresh-1")),
    )
}

fn query_of(url: &str) -> Vec<(String, String)> {
    pkce::parse_form(url.split_once('?').map_or("", |(_, query)| query))
}

fn value<'a>(pairs: &'a [(String, String)], name: &str) -> &'a str {
    pairs
        .iter()
        .find(|(key, _)| key == name)
        .map_or("", |(_, value)| value.as_str())
}

async fn bodies(server: &MockServer, wanted: &str, route: &str) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|request: &&Request| {
            request.method.as_str() == wanted && request.url.path() == route
        })
        .map(|request| serde_json::from_slice(&request.body).unwrap_or(serde_json::Value::Null))
        .collect()
}

#[tokio::test]
async fn login_trades_the_code_and_keeps_the_refresh_token() {
    let server = MockServer::start().await;
    token_endpoint(
        &server,
        "authorization_code",
        token("access-1", Some("refresh-1")),
    )
    .await;
    me_endpoint(&server, "access-1").await;
    let store = Arc::new(MemoryCredentials::new());
    let spotify = client(&server, store.clone());

    let login = spotify.start_login().await.expect("listening");
    let url = login.url().to_owned();
    assert!(url.starts_with(&format!("{}/authorize?", server.uri())));
    let query = query_of(&url);
    assert_eq!(value(&query, "client_id"), CLIENT_ID);
    assert_eq!(value(&query, "response_type"), "code");
    assert_eq!(value(&query, "code_challenge_method"), "S256");
    assert_eq!(value(&query, "scope"), pkce::SCOPES.join(" "));
    let redirect = value(&query, "redirect_uri").to_owned();
    assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/callback"));
    let state = value(&query, "state").to_owned();
    let challenge = value(&query, "code_challenge").to_owned();

    let browser = tokio::spawn(async move {
        let page = reqwest::get(format!("{redirect}?code=the%2Fcode&state={state}"))
            .await
            .expect("the listener answers");
        (
            page.status().as_u16(),
            page.text().await.unwrap_or_default(),
        )
    });
    let persistence = spotify
        .finish_login(login, &CallbackPages::default(), Duration::from_secs(10))
        .await
        .expect("logged in");
    assert_eq!(persistence, Persistence::System);
    let (status, page) = browser.await.expect("browser");
    assert_eq!(status, 200);
    assert!(
        page.contains("Connected to Spotify") && page.contains("Singline"),
        "{page}"
    );

    let sent = server.received_requests().await.unwrap_or_default();
    let form = pkce::parse_form(&String::from_utf8_lossy(&sent[0].body));
    assert_eq!(value(&form, "code"), "the/code");
    assert_eq!(value(&form, "client_id"), CLIENT_ID);
    assert_eq!(pkce::challenge(value(&form, "code_verifier")), challenge);
    assert!(value(&form, "redirect_uri").starts_with("http://127.0.0.1:"));

    assert_eq!(store.load(), Ok(Some("refresh-1".into())));
    assert_eq!(spotify.me().await.expect("me").name(), "Julian");
    assert_eq!(spotify.granted_all_scopes().await, Some(true));
}

#[tokio::test]
async fn without_a_credential_store_the_login_lasts_until_quit() {
    let server = MockServer::start().await;
    token_endpoint(
        &server,
        "authorization_code",
        token("access-1", Some("refresh-1")),
    )
    .await;
    me_endpoint(&server, "access-1").await;
    let spotify = client(&server, Arc::new(UnavailableCredentials));
    let persistence = spotify
        .exchange_code("code", "verifier", pkce::REDIRECT_URI)
        .await
        .expect("logged in");
    assert_eq!(persistence, Persistence::Memory);
    assert_eq!(spotify.persistence(), Some(Persistence::Memory));
    assert!(spotify.store_problem().is_some());
    assert!(spotify.has_login().await);
    assert_eq!(spotify.me().await.expect("me").id, "julian");
}

#[tokio::test]
async fn a_cancelled_login_ends_but_an_outdated_tab_does_not() {
    let server = MockServer::start().await;
    let spotify = client(&server, Arc::new(MemoryCredentials::new()));
    let login = spotify.start_login().await.expect("listening");
    let query = query_of(login.url());
    let redirect = value(&query, "redirect_uri").to_owned();
    let state = value(&query, "state").to_owned();

    let browser = tokio::spawn(async move {
        // A spare connection that never sends anything must not block the login.
        let _idle = tokio::net::TcpStream::connect(
            redirect
                .trim_start_matches("http://")
                .trim_end_matches("/callback"),
        )
        .await
        .expect("connects");
        let stale = reqwest::get(format!("{redirect}?code=old&state=from-before"))
            .await
            .expect("answer");
        let favicon = reqwest::get(redirect.replace("/callback", "/favicon.ico"))
            .await
            .expect("answer");
        let denied = reqwest::get(format!("{redirect}?error=access_denied&state={state}"))
            .await
            .expect("answer");
        (
            stale.status().as_u16(),
            stale.text().await.unwrap_or_default(),
            favicon.status().as_u16(),
            denied.status().as_u16(),
        )
    });
    let started = Instant::now();
    let outcome = spotify
        .finish_login(login, &CallbackPages::default(), Duration::from_secs(10))
        .await;
    assert_eq!(outcome, Err(LoginError::Denied));
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    let (stale, stale_page, favicon, denied) = browser.await.expect("browser");
    assert_eq!((stale, favicon, denied), (400, 404, 200));
    assert!(
        stale_page.contains("This login has expired"),
        "{stale_page}"
    );
    assert!(!spotify.has_login().await);
}

#[tokio::test]
async fn a_login_without_an_answer_times_out_and_frees_the_port() {
    let server = MockServer::start().await;
    let spotify = client(&server, Arc::new(MemoryCredentials::new()));
    let login = spotify.start_login().await.expect("listening");
    let redirect = value(&query_of(login.url()), "redirect_uri").to_owned();
    let outcome = spotify
        .finish_login(login, &CallbackPages::default(), Duration::from_millis(200))
        .await;
    assert_eq!(outcome, Err(LoginError::TimedOut));
    assert!(
        reqwest::get(redirect).await.is_err(),
        "the listener is gone"
    );
}

#[tokio::test]
async fn a_busy_port_is_reported() {
    let taken = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bound");
    let port = taken.local_addr().expect("address").port();
    let server = MockServer::start().await;
    let spotify = SpotifyClient::new(
        Config {
            callback_port: port,
            ..config(&server)
        },
        Arc::new(MemoryCredentials::new()),
    );
    let refused = spotify.start_login().await.err();
    assert_eq!(
        refused,
        Some(LoginError::Listen(spotify_connect::ListenError::PortInUse(
            port
        )))
    );
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh_and_keep_a_rotated_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=refresh-old"))
        .and(body_string_contains(format!("client_id={CLIENT_ID}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(token("fresh", Some("refresh-new")))
                .set_delay(Duration::from_millis(150)),
        )
        .expect(1)
        .mount(&server)
        .await;
    me_endpoint(&server, "fresh").await;
    let store = Arc::new(MemoryCredentials::with_secret("refresh-old"));
    let spotify = Arc::new(client(&server, store.clone()));

    let calls: Vec<_> = (0..5)
        .map(|_| {
            let spotify = Arc::clone(&spotify);
            tokio::spawn(async move { spotify.me().await })
        })
        .collect();
    for call in calls {
        assert_eq!(call.await.expect("task").expect("me").id, "julian");
    }
    assert_eq!(store.load(), Ok(Some("refresh-new".into())));
    assert_eq!(spotify.persistence(), Some(Persistence::System));
}

#[tokio::test]
async fn a_rejected_token_is_renewed_and_the_request_repeated() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token("stale", None)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    token_endpoint(&server, "refresh_token", token("fresh", None)).await;
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .and(header("authorization", "Bearer stale"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({"error": {"status": 401, "message": "The access token expired"}}),
        ))
        .mount(&server)
        .await;
    me_endpoint(&server, "fresh").await;
    let spotify = client(
        &server,
        Arc::new(MemoryCredentials::with_secret("refresh-1")),
    );
    assert_eq!(spotify.me().await.expect("renewed").id, "julian");
}

#[tokio::test]
async fn a_revoked_login_is_forgotten() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"error": "invalid_grant", "error_description": "Refresh token revoked"}),
        ))
        .mount(&server)
        .await;
    let store = Arc::new(MemoryCredentials::with_secret("refresh-1"));
    let spotify = client(&server, store.clone());
    assert_eq!(spotify.me().await.err(), Some(SpotifyError::LoginExpired));
    assert_eq!(store.load(), Ok(None));
    assert!(!spotify.has_login().await);
    assert_eq!(spotify.me().await.err(), Some(SpotifyError::NotConnected));

    let refused = client(&server, Arc::new(MemoryCredentials::new()))
        .exchange_code("bad", "verifier", pkce::REDIRECT_URI)
        .await;
    assert_eq!(
        refused,
        Err(SpotifyError::LoginRefused("Refresh token revoked".into()))
    );
}

#[tokio::test]
async fn waits_once_when_spotify_is_busy() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    me_endpoint(&server, "access-1").await;
    let started = Instant::now();
    assert!(spotify.me().await.is_ok());
    assert!(started.elapsed() >= Duration::from_secs(1));

    Mock::given(method("GET"))
        .and(path("/v1/me/player/devices"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
        .mount(&server)
        .await;
    assert_eq!(
        spotify.devices().await.err(),
        Some(SpotifyError::RateLimited {
            retry_after: Duration::from_secs(120)
        })
    );
}

#[tokio::test]
async fn likes_and_unlikes_songs() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    let uri = "spotify:track:4uLU6hMCjMI75M1A2tKUQC";
    Mock::given(method("GET"))
        .and(path("/v1/me/library/contains"))
        .and(query_param("uris", uri))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([true])))
        .mount(&server)
        .await;
    for verb in ["PUT", "DELETE"] {
        Mock::given(method(verb))
            .and(path("/v1/me/library"))
            .and(query_param("uris", uri))
            .and(header("content-length", "0"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
    }
    assert_eq!(spotify.is_saved(uri).await, Ok(true));
    assert_eq!(spotify.set_saved(uri, true).await, Ok(()));
    assert_eq!(spotify.set_saved(uri, false).await, Ok(()));
}

#[tokio::test]
async fn reads_top_lists_and_every_playlist() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    Mock::given(method("GET"))
        .and(path("/v1/me/top/artists"))
        .and(query_param("time_range", "short_term"))
        .and(query_param("limit", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            {"id": "a1", "name": "First", "uri": "spotify:artist:a1"}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/top/tracks"))
        .and(query_param("time_range", "long_term"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"items": [
            {"id": "t1", "name": "Song", "uri": "spotify:track:t1", "artists": [{"name": "First"}]}
        ]})))
        .mount(&server)
        .await;
    let artists = spotify
        .top(TopKind::Artists, TimeRange::Short, 10)
        .await
        .expect("artists");
    assert_eq!(artists[0].name, "First");
    let tracks = spotify
        .top(TopKind::Tracks, TimeRange::Long, 10)
        .await
        .expect("tracks");
    assert_eq!(tracks[0].artists, ["First"]);

    let playlist = |id: &str, owner: &str| json!({"id": id, "name": id, "owner": {"id": owner}, "collaborative": false});
    Mock::given(method("GET"))
        .and(path("/v1/me/playlists"))
        .and(query_param("offset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [playlist("p1", "julian"), playlist("p2", "spotify")],
            "next": "https://api.spotify.com/v1/me/playlists?offset=50&limit=50", "total": 3
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/playlists"))
        .and(query_param("offset", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "items": [playlist("p3", "julian")], "next": null, "total": 3
        })))
        .mount(&server)
        .await;
    let ids: Vec<String> = spotify
        .playlists()
        .await
        .expect("playlists")
        .into_iter()
        .map(|playlist| playlist.id)
        .collect();
    assert_eq!(ids, ["p1", "p2", "p3"]);
}

#[tokio::test]
async fn loads_every_page_of_a_source_in_order() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    let entry = |n: usize, key: &str| json!({ key: {"type": "track", "uri": format!("spotify:track:{n}"), "name": format!("Song {n}"), "artists": [{"id": "a", "name": "A"}]}});
    for offset in [0usize, 50, 100] {
        let liked: Vec<_> = (offset..(offset + 50).min(120))
            .map(|n| entry(n, "track"))
            .collect();
        Mock::given(method("GET"))
            .and(path("/v1/me/tracks"))
            .and(query_param("offset", offset.to_string()))
            .and(query_param("limit", "50"))
            .respond_with(
                ResponseTemplate::new(200)
                    // Later pages answer first; the order must still hold.
                    .set_delay(Duration::from_millis(if offset == 50 { 120 } else { 10 }))
                    .set_body_json(json!({"items": liked, "total": 120, "next": (offset < 100).then_some("more")})),
            )
            .expect(1)
            .mount(&server)
            .await;
    }
    let seen = Arc::new(Mutex::new(Vec::new()));
    let progress = Arc::clone(&seen);
    let tracks = spotify
        .tracks(LIKED_SONGS, move |loaded, total| {
            progress.lock().expect("lock").push((loaded, total))
        })
        .await
        .expect("liked songs");
    let numbers: Vec<String> = tracks.iter().map(|track| track.uri.clone()).collect();
    let expected: Vec<String> = (0..120).map(|n| format!("spotify:track:{n}")).collect();
    assert_eq!(numbers, expected);
    assert_eq!(
        *seen.lock().expect("lock"),
        [(50, 120), (100, 120), (120, 120)]
    );

    Mock::given(method("GET"))
        .and(path("/v1/playlists/PL1/items"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"items": [entry(7, "item")], "total": 1, "next": null})),
        )
        .mount(&server)
        .await;
    let items = spotify.tracks("PL1", |_, _| {}).await.expect("playlist");
    assert_eq!(items[0].uri, "spotify:track:7");
    assert_eq!(
        spotify.tracks("../me", |_, _| {}).await.err(),
        Some(SpotifyError::NotFound)
    );
}

#[tokio::test]
async fn starts_the_shuffle_on_a_device_in_order() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    Mock::given(method("GET"))
        .and(path("/v1/me/player/devices"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"devices": [
            {"id": "dev1", "name": "Laptop", "type": "Computer", "is_active": false}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v1/me/player/play"))
        .and(query_param("device_id", "dev1"))
        .and(body_json(
            json!({"context_uri": "spotify:playlist:PL1", "offset": {"position": 0}}),
        ))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    for (route, state) in [
        ("/v1/me/player/shuffle", "false"),
        ("/v1/me/player/repeat", "off"),
    ] {
        Mock::given(method("PUT"))
            .and(path(route))
            .and(query_param("state", state))
            .and(query_param("device_id", "dev1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
    }
    let devices = spotify.devices().await.expect("devices");
    let device = spotify_connect::model::preferred_device(&devices, None).expect("a computer");
    spotify
        .play_playlist("PL1", device.id.as_deref().unwrap_or_default())
        .await
        .expect("playing");

    Mock::given(method("GET"))
        .and(path("/v1/me/player/currently-playing"))
        .respond_with(ResponseTemplate::new(204))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/player/currently-playing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "context": {"uri": "spotify:playlist:PL1"}, "item": {"uri": "spotify:track:1"}, "is_playing": true
        })))
        .mount(&server)
        .await;
    assert_eq!(spotify.currently_playing().await, Ok(None));
    let playing = spotify
        .currently_playing()
        .await
        .expect("playing")
        .expect("something");
    assert!(playing.is_playlist("PL1"));
}

#[tokio::test]
async fn writes_playlists_in_batches_of_one_hundred() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    Mock::given(method("POST"))
        .and(path("/v1/me/playlists"))
        .and(body_json(json!({"name": "Singline Shuffle", "description": "Rewritten on every shuffle.", "public": false})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "NEW1"})))
        .mount(&server)
        .await;
    for verb in ["PUT", "POST", "DELETE"] {
        Mock::given(method(verb))
            .and(path("/v1/playlists/NEW1/items"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"snapshot_id": "s"})))
            .mount(&server)
            .await;
    }
    let id = spotify
        .create_playlist("Singline Shuffle", "Rewritten on every shuffle.")
        .await
        .expect("created");
    assert_eq!(id, "NEW1");
    let uris: Vec<String> = (0..250).map(|n| format!("spotify:track:{n}")).collect();
    let mut written = Vec::new();
    spotify
        .replace_items(&id, &uris, |count| written.push(count))
        .await
        .expect("written");
    assert_eq!(written, [100, 200, 250]);
    let replaced = bodies(&server, "PUT", "/v1/playlists/NEW1/items").await;
    let appended = bodies(&server, "POST", "/v1/playlists/NEW1/items").await;
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0]["uris"].as_array().map(Vec::len), Some(100));
    assert_eq!(replaced[0]["uris"][0], "spotify:track:0");
    let sizes: Vec<_> = appended
        .iter()
        .map(|body| body["uris"].as_array().map(Vec::len))
        .collect();
    assert_eq!(sizes, [Some(100), Some(50)]);
    assert_eq!(appended[1]["uris"][49], "spotify:track:249");

    spotify
        .append_items(&id, &uris[..3])
        .await
        .expect("appended");
    spotify
        .remove_items(&id, &["spotify:track:1".to_owned()])
        .await
        .expect("removed");
    let removed = bodies(&server, "DELETE", "/v1/playlists/NEW1/items").await;
    assert_eq!(removed, [json!({"items": [{"uri": "spotify:track:1"}]})]);

    // An empty order empties the playlist.
    spotify
        .replace_items(&id, &[], |_| {})
        .await
        .expect("emptied");
    let replaced = bodies(&server, "PUT", "/v1/playlists/NEW1/items").await;
    assert_eq!(replaced.last(), Some(&json!({"uris": []})));
}

#[tokio::test]
async fn refusals_and_failures_never_show_tokens() {
    let server = MockServer::start().await;
    let spotify = connected(&server).await;
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .respond_with(
            ResponseTemplate::new(403)
                .set_body_json(json!({"error": {"status": 403, "message": "Premium required"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/player/devices"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/me/player/currently-playing"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;
    let errors = [
        spotify.me().await.err(),
        spotify.devices().await.err(),
        spotify.currently_playing().await.err(),
    ];
    assert_eq!(
        errors,
        [
            Some(SpotifyError::Forbidden(Some("Premium required".into()))),
            Some(SpotifyError::NotFound),
            Some(SpotifyError::Http(503)),
        ]
    );
    for error in errors.iter().flatten() {
        let shown = format!("{error} {error:?}");
        assert!(
            !shown.contains("access-1") && !shown.contains("refresh-1"),
            "{shown}"
        );
    }

    let offline = SpotifyClient::new(
        Config {
            accounts_url: "http://127.0.0.1:9".into(),
            ..config(&server)
        },
        Arc::new(MemoryCredentials::with_secret("refresh-1")),
    );
    assert_eq!(offline.me().await.err(), Some(SpotifyError::Offline));
}
