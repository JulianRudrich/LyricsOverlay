//! The lyric data every other part of the app works with.

use serde::{Deserialize, Serialize};

/// One timed word or syllable inside a line. All times are seconds from the start
/// of the track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricWord {
    pub start: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    /// Includes the whitespace needed to rebuild the line by joining all words.
    pub text: String,
}

/// One lyric line. An empty `text` marks an instrumental break.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricLine {
    pub time: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    pub text: String,
    /// Empty when the source only has line timing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<LyricWord>,
}

impl LyricLine {
    pub fn new(time: f64, text: impl Into<String>) -> Self {
        Self {
            time,
            end: None,
            text: text.into(),
            words: Vec::new(),
        }
    }

    pub fn is_break(&self) -> bool {
        self.text.is_empty()
    }

    pub(crate) fn shifted(mut self, delta: f64) -> Self {
        self.time += delta;
        if let Some(end) = &mut self.end {
            *end += delta;
        }
        for word in &mut self.words {
            word.start += delta;
            if let Some(end) = &mut word.end {
                *end += delta;
            }
        }
        self
    }
}

/// How precisely a set of lyrics follows the music.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncLevel {
    /// Every word carries its own time.
    Word,
    /// Lines are timed, words are not.
    Line,
    /// Plain text without real timing.
    Unsynced,
}

/// Lyrics ready for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub sync: SyncLevel,
    pub instrumental: bool,
}

impl Lyrics {
    /// Wraps timed lines and derives the sync level from their content.
    pub fn timed(lines: Vec<LyricLine>) -> Self {
        let sync = if lines.iter().any(|line| !line.words.is_empty()) {
            SyncLevel::Word
        } else {
            SyncLevel::Line
        };
        Self {
            lines,
            sync,
            instrumental: false,
        }
    }

    /// Plain lyrics with estimated times.
    pub fn estimated(lines: Vec<LyricLine>) -> Self {
        Self {
            lines,
            sync: SyncLevel::Unsynced,
            instrumental: false,
        }
    }

    pub fn instrumental() -> Self {
        Self {
            lines: Vec::new(),
            sync: SyncLevel::Unsynced,
            instrumental: true,
        }
    }

    /// True when at least one line has words to show.
    pub fn has_text(&self) -> bool {
        self.lines.iter().any(|line| !line.is_break())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_level_follows_word_data() {
        let plain_line = LyricLine::new(1.0, "Hello");
        assert_eq!(
            Lyrics::timed(vec![plain_line.clone()]).sync,
            SyncLevel::Line
        );

        let mut worded = plain_line;
        worded.words.push(LyricWord {
            start: 1.0,
            end: None,
            text: "Hello".into(),
        });
        assert_eq!(Lyrics::timed(vec![worded]).sync, SyncLevel::Word);
    }

    #[test]
    fn breaks_do_not_count_as_text() {
        assert!(!Lyrics::timed(vec![LyricLine::new(1.0, "")]).has_text());
        assert!(Lyrics::timed(vec![LyricLine::new(1.0, "La")]).has_text());
        assert!(!Lyrics::instrumental().has_text());
    }
}
