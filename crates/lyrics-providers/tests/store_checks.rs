//! The `TrackLyricsStore` checks from the Swift app, ported to the SQLite store,
//! plus the cache.

use lyrics_core::lrc;
use lyrics_providers::{ImportError, Store};

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
