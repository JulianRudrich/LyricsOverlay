//! Sing to find against local pretend servers: the Genius search, the
//! duration-free LRCLIB lookup and the ranking of the candidates.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use lyrics_core::alignment::{self, HeardWord};
use lyrics_core::SyncLevel;
use lyrics_providers::{
    find_sung, GeniusClient, LookupOptions, LookupResult, LrclibClient, LyricsOutcome,
    LyricsService, SearchError, SongSearch, Store,
};
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const AGENT: &str = "Singline/test (https://github.com/JulianRudrich/LyricsOverlay)";
const TOKEN: &str = "test-token";

const RIGHT: &str = "\
[00:04.00]The river carries every name we gave the summer
[00:08.00]Paper boats are sailing past the silent harbor
[00:12.00]I can hear the morning calling from the mountain
[00:16.00]Every window opens when the swallows come home
";
const DECOY: &str = "\
[00:02.00]Good morning to the city
[00:06.00]The mountain is calling me
";

fn hit(title: &str, artist: &str) -> serde_json::Value {
    json!({
        "index": "song",
        "type": "song",
        "result": {
            "title": title,
            "full_title": format!("{title} by {artist}"),
            "artist_names": artist,
            "primary_artist": { "name": artist },
            "song_art_image_thumbnail_url": "https://images.genius.example/cover.png"
        }
    })
}

fn hits(entries: Vec<serde_json::Value>) -> serde_json::Value {
    json!({ "meta": { "status": 200 }, "response": { "hits": entries } })
}

fn record(title: &str, artist: &str, duration: f64, synced: Option<&str>) -> serde_json::Value {
    json!({
        "id": 1, "trackName": title, "artistName": artist, "albumName": "Album",
        "duration": duration, "instrumental": false, "plainLyrics": synced.map(|_| "plain"),
        "syncedLyrics": synced
    })
}

fn genius(server: &MockServer, token: &str) -> GeniusClient {
    GeniusClient::with_base_url(&server.uri(), token, AGENT)
}

/// The address of a port nothing listens on.
fn closed_url() -> String {
    let closed = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let address = closed.local_addr().expect("has an address");
    drop(closed);
    format!("http://{address}")
}

fn sung(text: &str, start: f64) -> Vec<HeardWord> {
    alignment::words(text)
        .into_iter()
        .enumerate()
        .map(|(index, word)| HeardWord::new(word, start + 0.35 * index as f64))
        .collect()
}

#[tokio::test]
async fn genius_finds_songs_by_their_words() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/search"))
        .and(query_param("q", "i can hear the morning calling"))
        .and(header("authorization", "Bearer test-token"))
        .and(header("user-agent", AGENT))
        .respond_with(ResponseTemplate::new(200).set_body_json(hits(vec![
            hit("Morning Song", "Example\u{a0}Artist"),
            hit(
                "Morning Song (English Translation)",
                "Genius English Translations",
            ),
            json!({ "type": "artist", "result": { "name": "Someone" } }),
            hit("morning song", "EXAMPLE ARTIST"),
            hit("Harbor", "Second Artist"),
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let found = genius(&server, TOKEN)
        .search("  i can hear the   morning calling ")
        .await
        .expect("search works");
    let names: Vec<_> = found
        .iter()
        .map(|song| (song.title.as_str(), song.artist.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("Morning Song", "Example Artist"),
            ("Harbor", "Second Artist")
        ],
        "translations, other kinds of pages and duplicates are left out"
    );
    assert_eq!(
        found[0].artwork_url.as_deref(),
        Some("https://images.genius.example/cover.png")
    );
}

#[tokio::test]
async fn genius_reports_why_it_cannot_search() {
    let server = MockServer::start().await;
    assert_eq!(
        genius(&server, " ").search_songs("some words").await,
        Err(SearchError::NoToken),
        "nothing is sent without a token"
    );
    assert_eq!(
        genius(&server, TOKEN).search_songs("   ").await,
        Ok(Vec::new())
    );
    assert!(server
        .received_requests()
        .await
        .unwrap_or_default()
        .is_empty());

    Mock::given(path("/search"))
        .and(query_param("q", "refused"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "meta": { "status": 401, "message": "invalid token" } })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/search"))
        .and(query_param("q", "busy"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "12"))
        .mount(&server)
        .await;
    Mock::given(path("/search"))
        .and(query_param("q", "broken"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    Mock::given(path("/search"))
        .and(query_param("q", "down"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;
    let client = genius(&server, TOKEN);
    assert_eq!(
        client.search_songs("refused").await,
        Err(SearchError::Unauthorized)
    );
    assert_eq!(
        client.search_songs("busy").await,
        Err(SearchError::RateLimited {
            retry_after: Duration::from_secs(12)
        })
    );
    assert_eq!(
        client.search_songs("broken").await,
        Err(SearchError::InvalidResponse)
    );
    assert_eq!(
        client.search_songs("down").await,
        Err(SearchError::Http(502))
    );

    let offline = GeniusClient::with_base_url(&closed_url(), TOKEN, AGENT);
    assert_eq!(
        offline.search_songs("anything").await,
        Err(SearchError::Offline)
    );
}

#[tokio::test]
async fn lrclib_finds_a_song_without_knowing_its_length() {
    let server = MockServer::start().await;
    let mut live = record("Harbor", "Example Artist", 260.0, Some("[00:01.00] Live"));
    live["albumName"] = json!("Live in Berlin");
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("track_name", "Harbor"))
        .and(query_param("artist_name", "Example Artist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            live,
            record(
                "Harbor (Remix)",
                "Example Artist",
                199.0,
                Some("[00:01.00] Remix")
            ),
            record("Harbor", "Example Artist", 201.0, Some("[00:01.00] Album")),
            record(
                "Harbor",
                "Example Artist feat. Guest",
                200.0,
                Some("[00:01.00] Single")
            ),
            record("Harbor", "Other Artist", 200.0, Some("[00:01.00] Cover")),
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(10)
        .mount(&server)
        .await;
    let client = LrclibClient::with_base_url(&server.uri(), AGENT).with_pause(Duration::ZERO);
    let found = client
        .find_song("Harbor", "Example Artist")
        .await
        .expect("lookup works");
    let LookupResult::Found { lyrics } = &found.result else {
        panic!("expected lyrics, got {found:?}");
    };
    assert_eq!(
        lyrics.lines[0].text, "Album",
        "the length two versions share wins over the live cut"
    );
    assert_eq!(found.duration, Some(201.0));
    assert_eq!(found.album.as_deref(), Some("Album"));

    let missing = client
        .find_song("Unknown", "Example Artist")
        .await
        .expect("lookup works");
    assert_eq!(missing.result, LookupResult::NotFound);
}

async fn lrclib_with_both_songs() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("track_name", "Morning Song"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([record(
            "Morning Song",
            "Example Artist",
            120.0,
            Some(RIGHT)
        )])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("track_name", "City Morning"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([record(
            "City Morning",
            "Decoy Band",
            90.0,
            Some(DECOY)
        )])))
        .mount(&server)
        .await;
    let mut plain = record("Plain Song", "Plain Artist", 100.0, None);
    plain["plainLyrics"] = json!("I can hear the morning\nCalling from the mountain");
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .and(query_param("track_name", "Plain Song"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([plain])))
        .mount(&server)
        .await;
    // Like LRCLIB, an unknown song gives an empty list.
    Mock::given(method("GET"))
        .and(path("/api/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(10)
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn the_candidate_whose_lyrics_explain_the_words_wins() {
    let genius_server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(hits(vec![
            hit("City Morning", "Decoy Band"),
            hit("Plain Song", "Plain Artist"),
            hit("Morning Song", "Example Artist"),
            hit("Not On Lrclib", "Nobody"),
        ])))
        .mount(&genius_server)
        .await;
    let lrclib = lrclib_with_both_songs().await;
    let store = Arc::new(Mutex::new(Store::open_in_memory().expect("store opens")));
    let service = LyricsService::new(
        LrclibClient::with_base_url(&lrclib.uri(), AGENT).with_pause(Duration::ZERO),
        Arc::clone(&store),
    );
    let search = genius(&genius_server, TOKEN);
    let heard = sung("i can hear the morning calling from the mountain", 30.0);
    let key = |title: &str, artist: &str| format!("key:{title}:{artist}");

    let found = find_sung(
        &search,
        &service,
        "i can hear the morning calling from the mountain",
        &heard,
        4,
        LookupOptions::default(),
        key,
    )
    .await
    .expect("search works");
    assert_eq!(found.candidates, 4);
    assert_eq!(found.lookup_problem, None);
    let titles: Vec<_> = found
        .matches
        .iter()
        .map(|found| found.candidate.title.as_str())
        .collect();
    assert_eq!(
        titles,
        ["Morning Song", "City Morning"],
        "plain lyrics cannot follow, and a song without lyrics is left out"
    );
    let best = &found.matches[0];
    assert!(best.alignment.is_confident());
    assert!(!found.matches[1].alignment.is_confident());
    assert_eq!(best.alignment.line, 2);
    assert!((best.alignment.offset - (12.0 - 30.0)).abs() < 0.01);
    assert_eq!(
        (best.duration, best.album.as_deref()),
        (Some(120.0), Some("Album"))
    );
    assert!(!best.own && best.lyrics.sync == SyncLevel::Line);

    // The second search is answered from the cache, and your own lyrics win.
    store
        .lock()
        .expect("store")
        .import_lyrics(
            "key:City Morning:Decoy Band",
            "[00:01.00]I can hear the morning calling from the mountain",
        )
        .expect("imported");
    let again = find_sung(
        &search,
        &service,
        "i can hear the morning",
        &heard,
        4,
        LookupOptions::default(),
        key,
    )
    .await
    .expect("search works");
    assert_eq!(again.matches[0].candidate.title, "City Morning");
    assert!(again.matches[0].own);
    let lrclib_requests = lrclib.received_requests().await.unwrap_or_default().len();
    assert_eq!(lrclib_requests, 4, "each song was looked up once");
}

#[tokio::test]
async fn lookups_stay_on_the_device_when_online_search_is_off() {
    let genius_server = MockServer::start().await;
    Mock::given(path("/search"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(hits(vec![hit("Morning Song", "Example Artist")])),
        )
        .mount(&genius_server)
        .await;
    let lrclib = MockServer::start().await;
    let service = LyricsService::new(
        LrclibClient::with_base_url(&lrclib.uri(), AGENT),
        Arc::new(Mutex::new(Store::open_in_memory().expect("store opens"))),
    );
    let lookup = service
        .song_lyrics(
            "key",
            "Morning Song",
            "Example Artist",
            LookupOptions {
                online: false,
                refresh: false,
            },
        )
        .await;
    assert_eq!(lookup.outcome, LyricsOutcome::LocalOnly);

    let offline_service = LyricsService::new(
        LrclibClient::with_base_url(&closed_url(), AGENT),
        Arc::new(Mutex::new(Store::open_in_memory().expect("store opens"))),
    );
    let offline = find_sung(
        &genius(&genius_server, TOKEN),
        &offline_service,
        "morning",
        &sung("morning calling", 0.0),
        4,
        LookupOptions::default(),
        |title: &str, _: &str| title.to_owned(),
    )
    .await
    .expect("the search itself works");
    assert!(offline.matches.is_empty());
    assert_eq!(offline.lookup_problem, Some(LyricsOutcome::Offline));
}
