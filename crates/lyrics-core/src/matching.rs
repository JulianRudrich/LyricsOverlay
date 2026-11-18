//! Conservative matching of lyric search results against the playing track.
//!
//! Showing the wrong song's lyrics is worse than showing none, so a result must
//! agree on title, artist and duration before it is accepted.

use serde::Deserialize;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::lrc;
use crate::lyricsfile::{self, Lyricsfile};
use crate::model::{Lyrics, SyncLevel};
use crate::plain;

/// Seconds a result's duration may differ from the playing track.
pub const DURATION_TOLERANCE: f64 = 5.0;

/// One record of the LRCLIB API.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LyricsRecord {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: Option<String>,
    pub duration: f64,
    pub instrumental: Option<bool>,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
    /// Word-level lyrics in the Lyricsfile format, when the record has them and
    /// they can be read.
    #[serde(deserialize_with = "lyricsfile::deserialize_lenient")]
    pub lyricsfile: Option<Lyricsfile>,
}

impl LyricsRecord {
    /// The best display form of this record: word timing, then line timing, then
    /// plain text spread over `duration` seconds, then the instrumental flag.
    pub fn to_lyrics(&self, duration: f64) -> Option<Lyrics> {
        let from_file = self
            .lyricsfile
            .as_ref()
            .and_then(|file| file.to_lyrics(Some(duration)));
        if let Some(lyrics) = from_file.as_ref().filter(|l| l.sync == SyncLevel::Word) {
            return Some(lyrics.clone());
        }
        if let Some(lines) = self.synced_lyrics.as_deref().map(lrc::parse) {
            if lines.iter().any(|line| !line.is_break()) {
                return Some(Lyrics::timed(lines));
            }
        }
        if let Some(lyrics) = from_file.filter(|l| l.sync == SyncLevel::Line) {
            return Some(lyrics);
        }
        if let Some(lines) = self
            .plain_lyrics
            .as_deref()
            .map(|text| plain::estimate(text, duration))
        {
            if !lines.is_empty() {
                return Some(Lyrics::estimated(lines));
            }
        }
        (self.instrumental == Some(true)).then(Lyrics::instrumental)
    }

    /// True when the record has line or word timing.
    pub fn has_synced_text(&self) -> bool {
        has_synced_text(self)
    }
}

/// Folds case and diacritics and reduces everything else to single spaces, so
/// "Beyoncé – Halo!" and "beyonce halo" compare equal.
pub fn normalized(value: &str) -> String {
    let folded: String = value
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect();
    folded
        .split(|c: char| !c.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when `record` describes the same recording as the playing track.
pub fn matches(record: &LyricsRecord, title: &str, artist: &str, duration: f64) -> bool {
    same_song(record, title, artist) && (record.duration - duration).abs() <= DURATION_TOLERANCE
}

/// True when `record` has this title and artist, in any length. A record may
/// name more artists after the first, as in "Artist feat. Guest".
pub fn same_song(record: &LyricsRecord, title: &str, artist: &str) -> bool {
    let title = normalized(title);
    let artist = normalized(artist);
    let candidate_artist = normalized(&record.artist_name);
    !title.is_empty()
        && !artist.is_empty()
        && normalized(&record.track_name) == title
        && (candidate_artist == artist
            || candidate_artist
                .strip_prefix(artist.as_str())
                .is_some_and(|rest| rest.starts_with(' ')))
}

/// The matching record with synced lyrics whose duration is closest to the track.
pub fn best<'a>(
    records: &'a [LyricsRecord],
    title: &str,
    artist: &str,
    duration: f64,
) -> Option<&'a LyricsRecord> {
    let mut best: Option<&LyricsRecord> = None;
    for record in records {
        if !matches(record, title, artist, duration) || !has_synced_text(record) {
            continue;
        }
        let closer = best.is_none_or(|current| {
            (record.duration - duration).abs() < (current.duration - duration).abs()
        });
        if closer {
            best = Some(record);
        }
    }
    best
}

fn has_synced_text(record: &LyricsRecord) -> bool {
    let in_lrc = record
        .synced_lyrics
        .as_deref()
        .is_some_and(|synced| lrc::parse(synced).iter().any(|line| !line.is_break()));
    in_lrc
        || record
            .lyricsfile
            .as_ref()
            .is_some_and(|file| file.lines.iter().any(|line| !line.text.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizing_folds_case_diacritics_and_punctuation() {
        assert_eq!(normalized("Beyoncé – Halo!"), "beyonce halo");
        assert_eq!(normalized("  Ärzte,   Die "), "arzte die");
        assert_eq!(normalized("!!!"), "");
    }

    #[test]
    fn artist_may_carry_featured_guests() {
        let record = LyricsRecord {
            track_name: "Song".into(),
            artist_name: "Artist feat. Guest".into(),
            duration: 200.0,
            ..Default::default()
        };
        assert!(matches(&record, "Song", "Artist", 200.0));
        assert!(
            !matches(&record, "Song", "Art", 200.0),
            "prefix must end at a word"
        );
    }

    #[test]
    fn equally_close_results_keep_the_first() {
        let make = |duration: f64, text: &str| LyricsRecord {
            track_name: "Song".into(),
            artist_name: "Artist".into(),
            duration,
            synced_lyrics: Some(format!("[00:01.00] {text}")),
            ..Default::default()
        };
        let records = [make(199.0, "first"), make(201.0, "second")];
        let chosen = best(&records, "Song", "Artist", 200.0).expect("a match exists");
        assert_eq!(chosen.synced_lyrics.as_deref(), Some("[00:01.00] first"));
    }

    const WORD_FILE: &str = "version: '1.0'\nlines:\n  - text: 'Hi there'\n    start_ms: 1000\n    words:\n      - {text: 'Hi ', start_ms: 1000}\n      - {text: 'there', start_ms: 1400}\n";

    #[test]
    fn word_timing_wins_over_line_timing() {
        let json = serde_json::json!({
            "trackName": "Song", "artistName": "Artist", "duration": 200,
            "syncedLyrics": "[00:01.00] Hi there", "plainLyrics": "Hi there",
            "lyricsfile": WORD_FILE, "hasWordSync": true
        });
        let record: LyricsRecord = serde_json::from_value(json).expect("valid record");
        let lyrics = record.to_lyrics(200.0).expect("has lyrics");
        assert_eq!(lyrics.sync, SyncLevel::Word);
        assert_eq!(lyrics.lines[0].words[1].start, 1.4);
    }

    #[test]
    fn lyricsfile_may_arrive_as_an_object() {
        let json = serde_json::json!({
            "trackName": "Song", "artistName": "Artist", "duration": 200,
            "lyricsfile": {"version": "1.0", "lines": [{"text": "Hi", "start_ms": 500}]}
        });
        let record: LyricsRecord = serde_json::from_value(json).expect("valid record");
        assert!(record.has_synced_text());
        assert_eq!(
            record.to_lyrics(200.0).map(|l| l.sync),
            Some(SyncLevel::Line)
        );
    }

    #[test]
    fn an_unreadable_lyricsfile_does_not_break_the_record() {
        for broken in [
            serde_json::json!(42),
            serde_json::json!("version: '9'"),
            serde_json::json!({"lines": "x"}),
        ] {
            let json = serde_json::json!({
                "trackName": "Song", "artistName": "Artist", "duration": 200,
                "syncedLyrics": "[00:01.00] Line", "lyricsfile": broken
            });
            let record: LyricsRecord = serde_json::from_value(json).expect("record still decodes");
            assert_eq!(record.lyricsfile, None);
            assert_eq!(
                record.to_lyrics(200.0).map(|l| l.sync),
                Some(SyncLevel::Line)
            );
        }
    }

    #[test]
    fn falls_back_to_plain_text_then_instrumental() {
        let plain = LyricsRecord {
            plain_lyrics: Some("First\nSecond".into()),
            ..Default::default()
        };
        assert_eq!(
            plain.to_lyrics(120.0).map(|l| l.sync),
            Some(SyncLevel::Unsynced)
        );
        assert_eq!(plain.to_lyrics(0.0), None, "plain text needs a duration");

        let instrumental = LyricsRecord {
            instrumental: Some(true),
            ..Default::default()
        };
        assert_eq!(instrumental.to_lyrics(120.0), Some(Lyrics::instrumental()));
        assert_eq!(LyricsRecord::default().to_lyrics(120.0), None);
    }
}
