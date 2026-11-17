//! Reading Lyricsfile documents, the YAML format LRCGET and LRCLIB use for
//! word-level lyrics. Follows the 1.0 draft at <https://lrclib.net/lyricsfile>.
//!
//! Reading is deliberately lenient where the draft leaves room (missing metadata,
//! end times before start times) and strict where it does not (unknown versions,
//! duplicate keys).

use std::cmp::Ordering;

use serde::{Deserialize, Deserializer};
use serde_yaml_ng::Value;
use thiserror::Error;

use crate::model::{LyricLine, LyricWord, Lyrics};
use crate::plain;

/// The largest document accepted, the same limit as for LRC files.
pub const MAX_BYTES: usize = 512 * 1_024;

/// The only version this reader understands.
pub const VERSION: &str = "1.0";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Lyricsfile {
    #[serde(deserialize_with = "version_string")]
    pub version: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub lines: Vec<Line>,
    #[serde(default)]
    pub plain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub duration_ms: Option<f64>,
    pub language: Option<String>,
    pub instrumental: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Line {
    pub text: String,
    pub start_ms: f64,
    #[serde(default)]
    pub end_ms: Option<f64>,
    #[serde(default)]
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Word {
    pub text: String,
    pub start_ms: f64,
    #[serde(default)]
    pub end_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LyricsfileError {
    #[error("the Lyricsfile is larger than {MAX_BYTES} bytes")]
    TooLarge,
    #[error("the Lyricsfile is not valid: {0}")]
    Invalid(String),
    #[error("unsupported Lyricsfile version {0:?}")]
    UnsupportedVersion(String),
}

/// Parses a Lyricsfile from YAML text.
pub fn parse(input: &str) -> Result<Lyricsfile, LyricsfileError> {
    if input.len() > MAX_BYTES {
        return Err(LyricsfileError::TooLarge);
    }
    let file: Lyricsfile = serde_yaml_ng::from_str(input)
        .map_err(|error| LyricsfileError::Invalid(error.to_string()))?;
    file.checked()
}

/// Reads a Lyricsfile that arrived already decoded, for example as a JSON object.
pub fn from_value(value: Value) -> Result<Lyricsfile, LyricsfileError> {
    let file: Lyricsfile = serde_yaml_ng::from_value(value)
        .map_err(|error| LyricsfileError::Invalid(error.to_string()))?;
    file.checked()
}

/// Deserializes an optional Lyricsfile given either as YAML text or as an object.
/// Anything unreadable becomes `None`, so one bad field never hides the rest of
/// an API record.
pub fn deserialize_lenient<'de, D>(deserializer: D) -> Result<Option<Lyricsfile>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(text) => parse(&text).ok(),
        Value::Mapping(_) => from_value(value).ok(),
        _ => None,
    })
}

impl Lyricsfile {
    fn checked(self) -> Result<Self, LyricsfileError> {
        if self.version == VERSION {
            Ok(self)
        } else {
            Err(LyricsfileError::UnsupportedVersion(self.version))
        }
    }

    /// True when at least one line carries word timing.
    pub fn has_word_sync(&self) -> bool {
        self.lines.iter().any(|line| !line.words.is_empty())
    }

    /// Converts the document for display. Synchronized lines win; otherwise the
    /// plain text is spread over the track. `track_duration` (seconds) takes
    /// precedence over the duration in the metadata.
    pub fn to_lyrics(&self, track_duration: Option<f64>) -> Option<Lyrics> {
        if self.metadata.instrumental {
            return Some(Lyrics::instrumental());
        }

        let mut lines: Vec<LyricLine> = self.lines.iter().filter_map(Line::to_lyric_line).collect();
        if lines.iter().any(|line| !line.is_break()) {
            lines.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(Ordering::Equal));
            return Some(Lyrics::timed(lines));
        }

        let duration = track_duration
            .filter(|duration| duration.is_finite() && *duration > 0.0)
            .or_else(|| self.metadata.duration_ms.map(|ms| ms / 1_000.0))?;
        let estimated = plain::estimate(self.plain.as_deref()?, duration);
        (!estimated.is_empty()).then(|| Lyrics::estimated(estimated))
    }
}

impl Line {
    fn to_lyric_line(&self) -> Option<LyricLine> {
        let start = seconds(self.start_ms)?;
        let words = self
            .words
            .iter()
            .filter_map(|word| {
                let start = seconds(word.start_ms)?;
                Some(LyricWord {
                    start,
                    end: end_after(word.end_ms, start),
                    text: word.text.clone(),
                })
            })
            .collect();
        Some(LyricLine {
            time: start,
            end: end_after(self.end_ms, start),
            text: self.text.trim().to_owned(),
            words,
        })
    }
}

fn seconds(milliseconds: f64) -> Option<f64> {
    (milliseconds.is_finite() && milliseconds >= 0.0).then(|| milliseconds / 1_000.0)
}

/// An end time only counts when it does not come before the start.
fn end_after(end_ms: Option<f64>, start: f64) -> Option<f64> {
    end_ms.and_then(seconds).filter(|end| *end >= start)
}

/// The draft requires a string, but an unquoted `1.0` is a common slip.
fn version_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::String(text) => Ok(text),
        Value::Number(number) if number.as_f64() == Some(1.0) => Ok(VERSION.to_owned()),
        Value::Number(number) => Ok(number.to_string()),
        _ => Err(serde::de::Error::custom("version must be a string")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SyncLevel;

    const WORD_SYNCED: &str = r#"
version: '1.0'
metadata:
  title: 'Paper Lanterns'
  artist: 'Example Artist'
  duration_ms: 180000
lines:
  - text: 'Light the way'
    start_ms: 1200
    end_ms: 2800
    words:
      - text: 'Light '
        start_ms: 1200
        end_ms: 1600
      - text: 'the '
        start_ms: 1600
        end_ms: 1900
      - text: 'way'
        start_ms: 1900
        end_ms: 2800
  - text: 'Home again'
    start_ms: 3000
"#;

    #[test]
    fn word_synced_documents_keep_word_times() {
        let file = parse(WORD_SYNCED).expect("valid document");
        assert!(file.has_word_sync());
        let lyrics = file.to_lyrics(None).expect("has lines");
        assert_eq!(lyrics.sync, SyncLevel::Word);
        assert_eq!(lyrics.lines.len(), 2);
        let first = &lyrics.lines[0];
        assert_eq!((first.time, first.end), (1.2, Some(2.8)));
        let words: Vec<_> = first
            .words
            .iter()
            .map(|w| (w.start, w.end, w.text.as_str()))
            .collect();
        assert_eq!(
            words,
            [
                (1.2, Some(1.6), "Light "),
                (1.6, Some(1.9), "the "),
                (1.9, Some(2.8), "way")
            ]
        );
        assert!(lyrics.lines[1].words.is_empty());
    }

    #[test]
    fn instrumental_documents_have_no_lines() {
        let file = parse(
            "version: '1.0'\nmetadata:\n  title: 'Calm'\n  artist: 'Band'\n  instrumental: true\n",
        )
        .expect("valid document");
        assert_eq!(file.to_lyrics(Some(200.0)), Some(Lyrics::instrumental()));
    }

    #[test]
    fn plain_only_documents_are_spread_over_the_track() {
        let file = parse("version: '1.0'\nmetadata:\n  title: 'T'\n  artist: 'A'\n  duration_ms: 120000\nplain: |\n  First line\n  Second line\n")
            .expect("valid document");
        let lyrics = file.to_lyrics(None).expect("plain text");
        assert_eq!(lyrics.sync, SyncLevel::Unsynced);
        assert_eq!(lyrics.lines.len(), 2);
        assert!(lyrics.lines[0].time < lyrics.lines[1].time);
        assert!(lyrics.lines[1].time < 120.0);
        assert_eq!(
            file.to_lyrics(Some(f64::NAN)).map(|l| l.lines.len()),
            Some(2),
            "falls back to the metadata duration"
        );
    }

    #[test]
    fn unknown_versions_are_rejected() {
        assert_eq!(
            parse("version: '2.0'\nmetadata: {title: 'T', artist: 'A'}\n"),
            Err(LyricsfileError::UnsupportedVersion("2.0".into()))
        );
        assert!(
            parse("version: 1.0\nmetadata: {title: 'T', artist: 'A'}\n").is_ok(),
            "unquoted 1.0"
        );
        assert!(matches!(
            parse("metadata: {title: 'T'}\n"),
            Err(LyricsfileError::Invalid(_))
        ));
    }

    #[test]
    fn duplicate_keys_and_oversized_input_are_rejected() {
        let duplicate = "version: '1.0'\nversion: '1.0'\nmetadata: {title: 'T', artist: 'A'}\n";
        assert!(matches!(parse(duplicate), Err(LyricsfileError::Invalid(_))));
        let large = format!("version: '1.0'\nplain: '{}'\n", "a".repeat(MAX_BYTES));
        assert_eq!(parse(&large), Err(LyricsfileError::TooLarge));
    }

    #[test]
    fn invalid_times_are_dropped_not_fatal() {
        let file = parse("version: '1.0'\nlines:\n  - {text: 'Back', start_ms: 5000, end_ms: 4000}\n  - {text: 'Never', start_ms: -10}\n")
            .expect("structure is valid");
        let lyrics = file.to_lyrics(None).expect("one usable line");
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!((lyrics.lines[0].time, lyrics.lines[0].end), (5.0, None));
    }
}
