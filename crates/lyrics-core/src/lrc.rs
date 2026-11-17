//! LRC parsing and editing.
//!
//! Supports repeated line timestamps (`[00:07.00][00:09.10] Again`), the
//! `[offset:±ms]` tag and enhanced LRC word tags (`<00:01.50>word`). Word tags are
//! kept as [`LyricWord`]s so the overlay can highlight word by word.

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{LyricLine, LyricWord};
use crate::text::{
    is_inline_space, is_line_padding, split_newlines, trim_inline, trim_padding, utf16_len,
    utf16_to_byte,
};

static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[([0-9]{1,3}):([0-9]{2}(?:\.[0-9]+)?)\]").expect("valid timestamp pattern")
});
static LEADING_TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[[0-9]{1,3}:[0-9]{2}(?:\.[0-9]+)?\]").expect("valid timestamp pattern")
});
static OFFSET_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\[offset:\s*([+-]?[0-9]+)\s*\]$").expect("valid offset pattern")
});
static WORD_TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<([0-9]{1,3}):([0-9]{2}(?:\.[0-9]+)?)>").expect("valid word pattern")
});
static METADATA_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[[A-Za-z][A-Za-z0-9_-]*:[^\]]*\]$").expect("valid metadata pattern")
});

/// Largest time an LRC timestamp can express: `[999:59.99]`.
const MAX_CENTISECONDS: f64 = (999 * 60 * 100 + 59 * 100 + 99) as f64;

/// Parses LRC text into lines sorted by time.
///
/// Lines without a valid timestamp are skipped. A timestamp with no text is kept
/// as an instrumental break. A positive `[offset:]` advances the lyrics, so the
/// first lines may get a negative time; they are kept so a user correction can
/// delay them again.
pub fn parse(input: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();
    let mut offset = 0.0;

    for raw in split_newlines(input) {
        let line = trim_padding(raw);
        if let Some(value) = parse_offset(line) {
            offset = value;
            continue;
        }

        let mut prefix_end = 0;
        let mut times = Vec::new();
        for captures in TIMESTAMP.captures_iter(line) {
            let token = captures.get(0).expect("group 0 always exists");
            // A timestamp inside the text belongs to the lyric, not the timing prefix.
            if !line[prefix_end..token.start()].chars().all(is_inline_space) {
                break;
            }
            prefix_end = token.end();
            if let Some(time) = clock_time(&captures[1], &captures[2]) {
                times.push(time);
            }
        }
        if times.is_empty() {
            continue;
        }

        let lyric = TimedText::parse(&line[prefix_end..]);
        lines.extend(times.iter().map(|&time| lyric.line_at(time, &times)));
    }

    // Stable, so lines sharing a timestamp keep their order in the file.
    lines.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(Ordering::Equal));
    lines
        .into_iter()
        .map(|line| line.shifted(-offset))
        .collect()
}

/// Index of the line that is active at `position`, or `None` before the first line.
/// `lines` must be sorted by time, as [`parse`] returns them.
pub fn current_index(lines: &[LyricLine], position: f64) -> Option<usize> {
    let (mut lower, mut upper) = (0, lines.len());
    while lower < upper {
        let middle = lower + (upper - lower) / 2;
        if lines[middle].time <= position {
            lower = middle + 1;
        } else {
            upper = middle;
        }
    }
    lower.checked_sub(1)
}

/// A text replacement that stamps one line of an LRC draft.
///
/// `start`, `length` and `next_caret` count UTF-16 code units, the unit in which
/// text fields report selections.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampEdit {
    pub start: usize,
    pub length: usize,
    pub replacement: String,
    /// Where the caret belongs after the edit: the start of the following line.
    pub next_caret: usize,
}

/// Stamps the line containing `caret` with the playback `position`.
///
/// `position` is the lyric position the listener currently hears, with the
/// overlay corrections applied. An existing leading timestamp is replaced. Returns
/// `None` for metadata lines such as `[ar:Artist]`, for a caret outside the draft
/// and for a position that is not a finite number.
pub fn timestamp_edit(draft: &str, caret: usize, position: f64) -> Option<TimestampEdit> {
    if !position.is_finite() {
        return None;
    }
    let caret = utf16_to_byte(draft, caret)?;
    let (start, contents_end, end) = line_bounds(draft, caret);
    let raw_line = &draft[start..contents_end];
    let line = trim_padding(raw_line);
    if METADATA_TAG.is_match(line) {
        return None;
    }

    let padding = raw_line.len() - raw_line.trim_start_matches(is_line_padding).len();
    // Timestamps are ASCII, so their byte length equals their UTF-16 length.
    let length = LEADING_TIMESTAMP.find(line).map_or(0, |token| token.end());
    let file_offset = split_newlines(draft)
        .filter_map(|raw| parse_offset(trim_padding(raw)))
        .last()
        .unwrap_or(0.0);

    let replacement = format_timestamp(position + file_offset);
    Some(TimestampEdit {
        start: utf16_len(&draft[..start + padding]),
        length,
        next_caret: utf16_len(&draft[..end]) + replacement.len() - length,
        replacement,
    })
}

/// Formats `seconds` as an LRC timestamp such as `[01:00.50]`, clamped to the
/// range the format can express.
pub fn format_timestamp(seconds: f64) -> String {
    let clamped = if seconds.is_finite() {
        seconds.clamp(0.0, MAX_CENTISECONDS / 100.0)
    } else {
        0.0
    };
    let centiseconds = (clamped * 100.0).round() as u64;
    format!(
        "[{:02}:{:02}.{:02}]",
        centiseconds / 6_000,
        (centiseconds / 100) % 60,
        centiseconds % 100
    )
}

fn parse_offset(line: &str) -> Option<f64> {
    let captures = OFFSET_TAG.captures(line)?;
    let milliseconds: i64 = captures[1].parse().ok()?;
    Some(milliseconds as f64 / 1_000.0)
}

fn clock_time(minutes: &str, seconds: &str) -> Option<f64> {
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    (seconds < 60.0).then_some(minutes * 60.0 + seconds)
}

/// The text of one LRC row with its word tags separated out.
struct TimedText {
    text: String,
    /// Pieces in order. `time` is the tag in front of the piece, `None` for text
    /// before the first tag.
    pieces: Vec<Piece>,
}

struct Piece {
    time: Option<f64>,
    text: String,
}

impl TimedText {
    fn parse(lyric: &str) -> Self {
        let mut pieces: Vec<Piece> = Vec::new();
        let mut cursor = 0;
        let mut pending: Option<f64> = None;
        let mut tagged = false;

        let push = |pieces: &mut Vec<Piece>, time: Option<f64>, text: &str, tagged: bool| {
            match (time, pieces.last_mut()) {
                // Text after an unreadable tag continues the previous piece.
                (None, Some(last)) if tagged => last.text.push_str(text),
                _ => pieces.push(Piece {
                    time,
                    text: text.to_owned(),
                }),
            }
        };

        for captures in WORD_TIMESTAMP.captures_iter(lyric) {
            let token = captures.get(0).expect("group 0 always exists");
            push(&mut pieces, pending, &lyric[cursor..token.start()], tagged);
            pending = clock_time(&captures[1], &captures[2]);
            tagged = true;
            cursor = token.end();
        }
        push(&mut pieces, pending, &lyric[cursor..], tagged);

        let text = trim_inline(
            &pieces
                .iter()
                .map(|piece| piece.text.as_str())
                .collect::<String>(),
        )
        .to_owned();

        if !tagged {
            pieces.clear();
        }
        Self { text, pieces }
    }

    /// Builds the line for one of the row's timestamps. Word tags hold absolute
    /// times for one occurrence; repeats are shifted by the distance between them.
    fn line_at(&self, time: f64, row_times: &[f64]) -> LyricLine {
        let mut line = LyricLine::new(time, self.text.clone());
        let Some(first_tag) = self.pieces.iter().find_map(|piece| piece.time) else {
            return line;
        };
        let anchor = row_times
            .iter()
            .copied()
            .min_by(|a, b| {
                (a - first_tag)
                    .abs()
                    .partial_cmp(&(b - first_tag).abs())
                    .unwrap_or(Ordering::Equal)
            })
            .unwrap_or(time);
        let shift = time - anchor;

        // Trim the outer whitespace the same way the line text is trimmed, so
        // joining the words reproduces the text.
        let mut texts: Vec<&str> = self
            .pieces
            .iter()
            .map(|piece| piece.text.as_str())
            .collect();
        if let Some(first) = texts.iter_mut().find(|text| !trim_inline(text).is_empty()) {
            *first = first.trim_start_matches(is_inline_space);
        }
        if let Some(last) = texts
            .iter_mut()
            .rev()
            .find(|text| !trim_inline(text).is_empty())
        {
            *last = last.trim_end_matches(is_inline_space);
        }

        let mut seen_text = false;
        for (index, piece) in self.pieces.iter().enumerate() {
            let text = texts[index];
            let blank = trim_inline(text).is_empty();
            if blank && !seen_text {
                continue; // leading whitespace before the first word
            }
            if blank {
                // A tag with nothing after it only closes the previous word.
                if let (Some(tag), Some(previous)) = (piece.time, line.words.last_mut()) {
                    previous
                        .end
                        .get_or_insert((tag + shift).max(previous.start));
                }
                continue;
            }
            seen_text = true;
            let start = piece.time.map_or(time, |tag| tag + shift);
            if let Some(previous) = line.words.last_mut() {
                previous.end.get_or_insert(start.max(previous.start));
            }
            line.words.push(LyricWord {
                start,
                end: None,
                text: text.to_owned(),
            });
        }
        line
    }
}

/// Start, content end and end (after the line break) of the line holding `caret`,
/// as byte offsets. A `\r\n` pair counts as one break.
fn line_bounds(text: &str, caret: usize) -> (usize, usize, usize) {
    let mut start = 0;
    loop {
        match next_line_break(&text[start..]) {
            Some((break_start, break_end)) => {
                let (contents_end, end) = (start + break_start, start + break_end);
                if caret < end {
                    return (start, contents_end, end);
                }
                start = end;
            }
            None => return (start, text.len(), text.len()),
        }
    }
}

fn next_line_break(text: &str) -> Option<(usize, usize)> {
    let mut chars = text.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        match c {
            '\r' => {
                let followed_by_lf = matches!(chars.peek(), Some((_, '\n')));
                return Some((index, index + 1 + usize::from(followed_by_lf)));
            }
            '\n' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                return Some((index, index + c.len_utf8()));
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(start: f64, end: Option<f64>, text: &str) -> LyricWord {
        LyricWord {
            start,
            end,
            text: text.to_owned(),
        }
    }

    #[test]
    fn word_tags_become_words() {
        let lines = parse("[00:10.00] <00:10.00>Hello <00:10.60>bright <00:11.20>world<00:12.00>");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].text, "Hello bright world");
        assert_eq!(
            lines[0].words,
            [
                word(10.0, Some(10.6), "Hello "),
                word(10.6, Some(11.2), "bright "),
                word(11.2, Some(12.0), "world"),
            ]
        );
        let joined: String = lines[0].words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(joined, lines[0].text);
    }

    #[test]
    fn lines_without_word_tags_have_no_words() {
        let lines = parse("[00:01.00] Plain line");
        assert!(lines[0].words.is_empty());
    }

    #[test]
    fn repeated_rows_shift_their_words() {
        let lines = parse("[00:01.00][00:03.00] <00:01.00>Second <00:01.50>line");
        assert_eq!(lines[0].words[1].start, 1.5);
        assert_eq!(lines[1].time, 3.0);
        assert_eq!(lines[1].words[0].start, 3.0);
        assert_eq!(lines[1].words[1].start, 3.5);
    }

    #[test]
    fn offset_moves_words_with_their_line() {
        let lines = parse("[offset:500]\n[00:02.00] <00:02.00>One <00:02.50>two");
        assert_eq!(lines[0].time, 1.5);
        assert_eq!(lines[0].words[0].start, 1.5);
        assert_eq!(lines[0].words[0].end, Some(2.0));
        assert_eq!(lines[0].words[1].start, 2.0);
    }

    #[test]
    fn text_before_the_first_tag_starts_with_the_line() {
        let lines = parse("[00:05.00] Oh <00:05.40>yeah");
        assert_eq!(
            lines[0].words,
            [word(5.0, Some(5.4), "Oh "), word(5.4, None, "yeah")]
        );
    }

    #[test]
    fn syllables_keep_their_spacing() {
        let lines = parse("[00:01.00]<00:01.00>Mor<00:01.20>gen<00:01.40>licht");
        let texts: Vec<_> = lines[0].words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(texts, ["Mor", "gen", "licht"]);
        assert_eq!(lines[0].text, "Morgenlicht");
    }

    #[test]
    fn unreadable_word_tags_are_dropped_from_the_text() {
        let lines = parse("[00:01.00] <00:01.00>Stay <00:99.00>here");
        assert_eq!(lines[0].text, "Stay here");
        assert_eq!(lines[0].words, [word(1.0, None, "Stay here")]);
    }

    #[test]
    fn formats_and_clamps_timestamps() {
        assert_eq!(format_timestamp(0.0), "[00:00.00]");
        assert_eq!(format_timestamp(60.499), "[01:00.50]");
        assert_eq!(format_timestamp(-4.0), "[00:00.00]");
        assert_eq!(format_timestamp(1e9), "[999:59.99]");
        assert_eq!(format_timestamp(f64::NAN), "[00:00.00]");
    }

    #[test]
    fn stamping_between_cr_and_lf_stays_on_that_line() {
        let draft = "One\r\nTwo";
        let edit = timestamp_edit(draft, 4, 1.0).expect("caret sits on the first line");
        assert_eq!((edit.start, edit.length), (0, 0));
        assert_eq!(edit.next_caret, 5 + edit.replacement.len());
    }

    #[test]
    fn stamping_rejects_a_caret_inside_a_surrogate_pair() {
        assert_eq!(timestamp_edit("🌙", 1, 1.0), None);
    }
}
