//! The `TrackLyricsStore` checks from the Swift app, ported to the SQLite store,
//! plus the cache.

use lyrics_core::listening::ListenSession;
use lyrics_core::lrc;
use lyrics_providers::{ImportError, Store, StoreError};

const IMPORTED: &str = "\u{FEFF}[ar:Example]\r\n[00:00.25] First\r\n[00:01.00] [00:03.00] <00:01.00>Second <00:01.50>line\r\n[00:02.00]\r\n [OFFSET: +500] ";

fn temporary_path(name: &str) -> std::path::PathBuf {
    let unique = format!(
        "singline-{name}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}

struct Cleanup(std::path::PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

#[test]
fn keeps_offsets_and_own_lyrics_per_track() {
    let path = temporary_path("store");
    let _cleanup = Cleanup(path.clone());
    let shifted = lrc::parse(IMPORTED);
    let track = "spotify:track:first";
    let other_track = "spotify:track:second";

    {
        let store = Store::open(&path).expect("store opens");
        assert_eq!(store.timing_offset(track), 0.0);
        store.set_timing_offset(track, 0.25).expect("saved");
        store.set_timing_offset(other_track, -0.5).expect("saved");
        let saved = store.import_lyrics(track, IMPORTED).expect("valid LRC");
        assert_eq!(saved, shifted);

        assert_eq!(
            store.import_lyrics(track, "[00:01.00] \nNo timed words"),
            Err(ImportError::MissingTimedLyrics),
            "Import must reject LRC with no timed text"
        );
        assert_eq!(
            store.local_lyrics(track).as_deref().map(lrc::parse),
            Some(shifted.clone()),
            "Failed import keeps the previous text"
        );

        let oversized = format!("[00:00.00]{}", "ä".repeat(262_144));
        assert_eq!(
            store.import_lyrics(track, &oversized),
            Err(ImportError::TooLarge),
            "Oversized UTF-8 drafts must be rejected"
        );
        assert_eq!(
            store.local_lyrics(track).as_deref().map(lrc::parse),
            Some(shifted.clone())
        );
    }

    let reopened = Store::open(&path).expect("store reopens");
    assert_eq!(reopened.timing_offset(track), 0.25);
    assert_eq!(
        reopened.local_lyrics(track).as_deref().map(lrc::parse),
        Some(shifted.clone())
    );
    assert_eq!(reopened.timing_offset(other_track), -0.5);
    assert_eq!(reopened.local_lyrics(other_track), None);
    reopened.remove_local_lyrics(track).expect("removed");
    reopened
        .remove_local_lyrics(track)
        .expect("removing twice is fine");
    assert_eq!(reopened.local_lyrics(track), None);
    assert_eq!(
        reopened.timing_offset(track),
        0.25,
        "removing lyrics keeps the timing"
    );
    reopened.set_timing_offset(track, 0.0).expect("reset");
    assert_eq!(reopened.timing_offset(track), 0.0);
    assert_eq!(reopened.timing_offset(other_track), -0.5);

    let local_track = format!("spotify:local:../a/b/{}", "Ü".repeat(150));
    reopened
        .import_lyrics(&local_track, IMPORTED)
        .expect("long keys work");
    assert_eq!(
        reopened
            .local_lyrics(&local_track)
            .as_deref()
            .map(lrc::parse),
        Some(shifted)
    );
    assert_eq!(
        reopened.local_lyrics(&local_track.to_lowercase()),
        None,
        "keys are case-sensitive"
    );
    reopened.remove_local_lyrics(&local_track).expect("removed");
}

#[test]
fn non_finite_offsets_are_ignored() {
    let store = Store::open_in_memory().expect("store opens");
    store.set_timing_offset("t", 1.5).expect("saved");
    store.set_timing_offset("t", f64::NAN).expect("ignored");
    assert_eq!(store.timing_offset("t"), 1.5);
}

#[test]
fn cache_expires_and_stays_bounded() {
    let store = Store::open_in_memory().expect("store opens");
    store.store_cached("a", &"first", 100, 200).expect("cached");
    assert_eq!(store.cached::<String>("a", 150).as_deref(), Some("first"));
    assert_eq!(
        store.cached::<String>("a", 200),
        None,
        "expired at its deadline"
    );
    store
        .store_cached("a", &"second", 300, 400)
        .expect("replaced");
    assert_eq!(store.cached::<String>("a", 350).as_deref(), Some("second"));
    store.forget_cached("a").expect("forgotten");
    assert_eq!(store.cached::<String>("a", 350), None);

    for index in 0..2_005 {
        store
            .store_cached(&format!("k{index}"), &index, 1_000 + index, 10_000_000)
            .expect("cached");
    }
    assert_eq!(store.cache_len(), 2_000);
    assert_eq!(
        store.cached::<i64>("k0", 5_000),
        None,
        "least recently used went first"
    );
    assert_eq!(store.cached::<i64>("k2004", 5_000), Some(2_004));
    store.clear_cache().expect("cleared");
    assert_eq!(store.cache_len(), 0);
}

fn listen(key: &str, started_at: i64, listened: f64) -> ListenSession {
    ListenSession {
        track_key: key.into(),
        title: format!("Title {key}"),
        artist: "Artist".into(),
        album: Some("Album".into()),
        service: "spotify".into(),
        started_at,
        duration: Some(200.0),
        listened,
        synced_lyrics: true,
    }
}

fn file_contains(path: &std::path::Path, suffix: &str, needle: &str) -> bool {
    std::fs::read(format!("{}{suffix}", path.display())).is_ok_and(|bytes| {
        bytes
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    })
}

#[test]
fn keeps_the_listening_history_until_it_is_cleared() {
    let path = temporary_path("history");
    let _cleanup = Cleanup(path.clone());
    {
        let store = Store::open(&path).expect("store opens");
        store.add_listen(&listen("b", 2_000, 45.5)).expect("saved");
        store.add_listen(&listen("a", 1_000, 12.0)).expect("saved");
        let mut odd = listen("c", 3_000, f64::NAN);
        odd.album = None;
        odd.duration = Some(f64::INFINITY);
        store.add_listen(&odd).expect("saved");
    }

    let store = Store::open(&path).expect("store reopens");
    let all = store.listens(None).expect("readable");
    assert_eq!(
        all.iter()
            .map(|session| session.track_key.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"],
        "oldest first"
    );
    assert_eq!(all[1], listen("b", 2_000, 45.5));
    assert_eq!(
        (all[2].listened, all[2].duration, all[2].album.as_deref()),
        (0.0, None, None),
        "impossible values are stored as unknown"
    );
    assert_eq!(store.listens(Some(2_000)).expect("readable").len(), 2);
    assert_eq!(store.listen_count(), 3);
    assert!(file_contains(&path, "", "Title b"));

    store.clear_listens().expect("cleared");
    assert_eq!(store.listen_count(), 0);
    assert!(store.listens(None).expect("readable").is_empty());
    for suffix in ["", "-wal"] {
        assert!(
            !file_contains(&path, suffix, "Title b"),
            "cleared songs are gone from the file{suffix}"
        );
    }
    store
        .add_listen(&listen("d", 4_000, 30.0))
        .expect("recording goes on after clearing");
    assert_eq!(store.listen_count(), 1);
}

#[test]
fn a_database_from_version_one_keeps_its_data() {
    let path = temporary_path("version-one");
    let _cleanup = Cleanup(path.clone());
    {
        let old = rusqlite::Connection::open(&path).expect("opens");
        old.execute_batch(
            "CREATE TABLE track_settings (track_key TEXT PRIMARY KEY, timing_offset REAL NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE local_lyrics (track_key TEXT PRIMARY KEY, lrc TEXT NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE lyrics_cache (query_key TEXT PRIMARY KEY, result TEXT NOT NULL, expires_at INTEGER NOT NULL, last_used INTEGER NOT NULL);
             CREATE INDEX lyrics_cache_last_used ON lyrics_cache (last_used);
             INSERT INTO track_settings VALUES ('song', 0.75, 1);
             INSERT INTO local_lyrics VALUES ('song', '[00:01.00] Hello', 1);
             INSERT INTO lyrics_cache VALUES ('query', '\"cached\"', 4000000000, 1);
             PRAGMA user_version = 1;",
        )
        .expect("a version 1 database");
    }

    let store = Store::open(&path).expect("updates");
    assert_eq!(store.timing_offset("song"), 0.75);
    assert_eq!(
        store.local_lyrics("song").as_deref(),
        Some("[00:01.00] Hello")
    );
    assert_eq!(
        store.cached::<String>("query", 100).as_deref(),
        Some("cached")
    );
    assert!(store
        .listens(None)
        .expect("the new table exists")
        .is_empty());
    store
        .add_listen(&listen("a", 1_000, 60.0))
        .expect("recording works after the update");
    drop(store);

    let version: i32 = rusqlite::Connection::open(&path)
        .and_then(|reopened| reopened.query_row("PRAGMA user_version", [], |row| row.get(0)))
        .expect("readable");
    assert_eq!(version, 2);
    let again = Store::open(&path).expect("opens again");
    assert_eq!(again.listen_count(), 1, "opening twice changes nothing");
    assert_eq!(again.timing_offset("song"), 0.75);
}

#[test]
fn a_database_from_a_newer_version_is_left_alone() {
    let path = temporary_path("newer");
    let _cleanup = Cleanup(path.clone());
    rusqlite::Connection::open(&path)
        .and_then(|newer| newer.execute_batch("PRAGMA user_version = 3;"))
        .expect("a newer database");
    assert!(matches!(Store::open(&path), Err(StoreError::NewerSchema)));
}
