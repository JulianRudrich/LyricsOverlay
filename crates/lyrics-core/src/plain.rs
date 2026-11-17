//! Spreading plain, untimed lyrics over a track so they can still move with the
//! music. The result is an estimate and is always shown as "not synced".
//!
//! Longer lines get more time than short ones, and a blank line between stanzas
//! adds a short pause. Section labels such as `[Chorus]` are skipped.

use crate::model::LyricLine;
use crate::text::{split_newlines, trim_padding};

/// Share of the track before the first line, capped at [`MAX_LEAD`] seconds.
const LEAD_SHARE: f64 = 0.06;
const MAX_LEAD: f64 = 12.0;
/// Share of the track after the last line starts.
const TAIL_SHARE: f64 = 0.06;
/// Weight every line gets on top of its length, so short lines stay readable.
const LINE_BASE_WEIGHT: f64 = 6.0;
/// Weight of the pause between two stanzas.
const STANZA_GAP_WEIGHT: f64 = 10.0;

/// Estimated timing for plain lyrics on a track of `duration` seconds. Returns no
/// lines for empty text or an unknown duration.
pub fn estimate(text: &str, duration: f64) -> Vec<LyricLine> {
    if !duration.is_finite() || duration <= 0.0 {
        return Vec::new();
    }

    // (text, pause before it)
    let mut rows: Vec<(&str, bool)> = Vec::new();
    let mut pause = false;
    for raw in split_newlines(text) {
        let line = trim_padding(raw);
        if line.is_empty() || is_section_label(line) {
            pause = !rows.is_empty();
            continue;
        }
        rows.push((line, pause));
        pause = false;
    }
    if rows.is_empty() {
        return Vec::new();
    }

    let weight = |line: &str| line.chars().count() as f64 + LINE_BASE_WEIGHT;
    let total: f64 = rows
        .iter()
        .map(|(line, pause)| weight(line) + if *pause { STANZA_GAP_WEIGHT } else { 0.0 })
        .sum();
    let lead = (duration * LEAD_SHARE).min(MAX_LEAD);
    let available = (duration - lead - duration * TAIL_SHARE).max(0.0);

    let mut cursor = 0.0;
    rows.into_iter()
        .map(|(line, pause)| {
            if pause {
                cursor += STANZA_GAP_WEIGHT;
            }
            let time = lead + cursor / total * available;
            cursor += weight(line);
            LyricLine::new(time, line)
        })
        .collect()
}

/// `[Chorus]`, `[Verse 2: Artist]` and similar headings that are not sung.
fn is_section_label(line: &str) -> bool {
    line.len() > 2
        && line.starts_with('[')
        && line.ends_with(']')
        && !line[1..line.len() - 1].contains(['[', ']'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn times(lines: &[LyricLine]) -> Vec<f64> {
        lines.iter().map(|line| line.time).collect()
    }

    #[test]
    fn nothing_to_spread() {
        assert!(estimate("", 200.0).is_empty());
        assert!(estimate("\n  \n[Intro]\n", 200.0).is_empty());
        assert!(estimate("Line", 0.0).is_empty());
        assert!(estimate("Line", f64::NAN).is_empty());
    }

    #[test]
    fn lines_stay_inside_the_track_in_order() {
        let lines = estimate("One\nTwo\nThree\nFour", 200.0);
        let times = times(&lines);
        assert_eq!(lines.len(), 4);
        assert!((times[0] - 12.0).abs() < 1e-9, "lead is 6 % of 200 s");
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(*times.last().expect("four lines") < 200.0 * (1.0 - TAIL_SHARE));
    }

    #[test]
    fn long_lines_get_more_time() {
        let lines = estimate(
            "A much longer line with many words in it\nShort\nEnd",
            180.0,
        );
        let times = times(&lines);
        assert!(times[1] - times[0] > times[2] - times[1]);
    }

    #[test]
    fn stanza_breaks_add_a_pause_and_labels_are_skipped() {
        let joined = estimate("Same\nSame\nSame", 180.0);
        let split = estimate("Same\nSame\n\n[Chorus]\nSame", 180.0);
        assert_eq!(split.len(), 3);
        assert!(split.iter().all(|line| line.text == "Same"));
        let gap = |lines: &[LyricLine], index: usize| lines[index + 1].time - lines[index].time;
        assert!(gap(&split, 1) > gap(&split, 0), "the stanza gap is longer");
        assert!(gap(&split, 1) > gap(&joined, 1));
    }

    #[test]
    fn brackets_inside_lyrics_are_kept() {
        let lines = estimate("[Hook] Say it [again]\n(Oh)", 100.0);
        assert_eq!(lines.len(), 2);
    }
}
