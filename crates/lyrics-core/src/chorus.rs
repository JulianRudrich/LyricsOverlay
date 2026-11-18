//! Finding the chorus: the group of lines a song repeats most often, so the
//! overlay can mark where each chorus starts and jump there.

use std::cmp::Reverse;
use std::collections::HashMap;

use crate::matching::normalized;
use crate::model::LyricLine;

/// A chorus start must lie more than this many seconds ahead to be the next
/// one, so a second jump right after the first moves on instead of landing on
/// the start it just reached.
const AHEAD: f64 = 0.5;

/// Start times of every occurrence of the chorus, earliest first, or nothing
/// when no group of lines repeats.
///
/// The chorus is the group of at least two consecutive sung lines that occurs
/// most often without overlapping itself. Among groups that occur equally
/// often the longer one wins, because it is the whole chorus rather than a part
/// of it, then the one sung first. Lines compare by their normalized text, so
/// case, accents and punctuation do not matter. Breaks and lines without words
/// are skipped: they neither belong to a chorus nor split one.
pub fn starts(lines: &[LyricLine]) -> Vec<f64> {
    let (times, texts) = sung_lines(lines);
    let mut best: Option<Group> = None;
    // Two separate occurrences need room for twice the group.
    for length in 2..=texts.len() / 2 {
        // Groups this long no longer fit in often enough to beat the best one.
        if best
            .as_ref()
            .is_some_and(|group| texts.len() / length < group.starts.len())
        {
            break;
        }
        let mut occurrences: HashMap<&[usize], Vec<usize>> = HashMap::new();
        for (start, window) in texts.windows(length).enumerate() {
            occurrences.entry(window).or_default().push(start);
        }
        let mut repeats = false;
        for found in occurrences.into_values().filter(|found| found.len() > 1) {
            repeats = true;
            let group = Group::new(length, &found);
            let better = best
                .as_ref()
                .is_none_or(|current| group.rank() > current.rank());
            if group.starts.len() > 1 && better {
                best = Some(group);
            }
        }
        // A longer group repeats only where its first `length` lines repeat.
        if !repeats {
            break;
        }
    }
    best.map(|group| group.starts.iter().map(|&index| times[index]).collect())
        .unwrap_or_default()
}

/// The chorus to jump to from `position`: the first start more than half a
/// second ahead, or the first chorus once the last one has begun. `starts`
/// must be sorted, as [`starts`] returns them. Positions are lyric times.
pub fn next_after(position: f64, starts: &[f64]) -> Option<f64> {
    starts
        .iter()
        .copied()
        .find(|&start| start > position + AHEAD)
        .or_else(|| starts.first().copied())
}

/// Times and texts of the sung lines. Equal texts get the same number, so
/// groups of lines compare as plain number slices.
fn sung_lines(lines: &[LyricLine]) -> (Vec<f64>, Vec<usize>) {
    let mut numbers: HashMap<String, usize> = HashMap::new();
    let mut times = Vec::new();
    let mut texts = Vec::new();
    for line in lines {
        let text = normalized(&line.text);
        if text.is_empty() {
            continue;
        }
        let next = numbers.len();
        texts.push(*numbers.entry(text).or_insert(next));
        times.push(line.time);
    }
    (times, texts)
}

/// A group of `length` lines and where it starts, without overlaps.
struct Group {
    length: usize,
    starts: Vec<usize>,
}

impl Group {
    /// Keeps every occurrence that begins after the last kept one has ended.
    /// Taking the earliest each time keeps as many as can be kept.
    fn new(length: usize, occurrences: &[usize]) -> Self {
        let mut starts: Vec<usize> = Vec::new();
        for &start in occurrences {
            if starts.last().is_none_or(|&last| start >= last + length) {
                starts.push(start);
            }
        }
        Self { length, starts }
    }

    /// Higher is better: more occurrences, then more lines, then sung earlier.
    fn rank(&self) -> (usize, usize, Reverse<usize>) {
        let first = self.starts.first().copied().unwrap_or(usize::MAX);
        (self.starts.len(), self.length, Reverse(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One line every four seconds, starting at four.
    fn song(texts: &[&str]) -> Vec<LyricLine> {
        texts
            .iter()
            .enumerate()
            .map(|(index, text)| LyricLine::new(4.0 * (index + 1) as f64, *text))
            .collect()
    }

    /// The times of the lines at `indices`.
    fn times_of(lines: &[LyricLine], indices: &[usize]) -> Vec<f64> {
        indices.iter().map(|&index| lines[index].time).collect()
    }

    #[test]
    fn finds_every_chorus_of_a_song() {
        let lines = song(&[
            "Paper boats on Sunday",
            "Drifting past the mill",
            "Hold the lantern higher",
            "Let the river sing",
            "Hold the lantern higher!",
            "Copper leaves in autumn",
            "Falling on the hill",
            "HOLD THE LANTERN HIGHER",
            "Let the river sing...",
            "hold the lantern higher",
            "",
            "Quiet now",
            "Quiet now",
            "Hold the lantern higher",
            "Let the river sing",
            "Hold the lantern higher",
        ]);
        assert_eq!(starts(&lines), times_of(&lines, &[2, 7, 13]));
    }

    #[test]
    fn nothing_repeats() {
        assert!(starts(&[]).is_empty());
        let unique: Vec<String> = (1..=40).map(|n| format!("Line number {n}")).collect();
        let texts: Vec<&str> = unique.iter().map(String::as_str).collect();
        assert!(starts(&song(&texts)).is_empty());
    }

    #[test]
    fn one_repeated_line_is_not_a_chorus() {
        let lines = song(&["Echo", "Other", "Echo", "More", "Echo", "Last"]);
        assert!(starts(&lines).is_empty());
    }

    #[test]
    fn two_lines_are_enough() {
        let lines = song(&["Come home", "Come home now", "Come home", "Come home now"]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 2]));
        assert!(
            starts(&song(&["A", "B", "A"])).is_empty(),
            "three lines hold no two separate groups"
        );
    }

    #[test]
    fn the_most_repeated_group_wins() {
        // A short hook sung three times beats a longer group sung twice.
        let lines = song(&[
            "Na na hey",
            "Na na ho",
            "Verse one here",
            "Then a longer part",
            "Na na hey",
            "Na na ho",
            "Verse one here",
            "Then a longer part",
            "Na na hey",
            "Na na ho",
            "End",
        ]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 4, 8]));
    }

    #[test]
    fn equally_often_the_whole_chorus_wins_over_its_parts() {
        let chorus = [
            "One more night",
            "Under the lights",
            "Sing it loud",
            "Sing it proud",
        ];
        let mut texts = vec!["Verse A", "Verse B"];
        texts.extend(chorus);
        texts.extend(["Verse C", "Verse D"]);
        texts.extend(chorus);
        texts.push("Bridge");
        texts.extend(chorus);
        let lines = song(&texts);
        assert_eq!(starts(&lines), times_of(&lines, &[2, 8, 13]));
    }

    #[test]
    fn equally_often_and_long_the_earlier_group_wins() {
        let lines = song(&[
            "A1", "A2", "x", "B1", "B2", "y", "A1", "A2", "z", "B1", "B2",
        ]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 6]));
        let swapped = song(&[
            "B1", "B2", "y", "A1", "A2", "x", "B1", "B2", "z", "A1", "A2",
        ]);
        assert_eq!(starts(&swapped), times_of(&swapped, &[0, 6]));
    }

    #[test]
    fn ties_never_depend_on_hashing() {
        // Eight different pairs, each sung twice, the second time in reverse
        // order of pairs: always the pair sung first.
        let pairs: Vec<String> = (0..8)
            .flat_map(|n| [format!("Pair {n} a"), format!("Pair {n} b")])
            .collect();
        let mut texts: Vec<&str> = pairs.iter().map(String::as_str).collect();
        texts.push("Middle");
        texts.extend(pairs.chunks(2).rev().flatten().map(String::as_str));
        let lines = song(&texts);
        for _ in 0..20 {
            assert_eq!(starts(&lines), times_of(&lines, &[0, 31]));
        }
    }

    #[test]
    fn occurrences_do_not_overlap() {
        // "La, La" starts at 0, 1 and 4, but 1 would reuse the line at 1.
        let lines = song(&["La", "La", "La", "Ho", "La", "La"]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 4]));

        let pairs = song(&["Oh", "Yeah", "Oh", "Yeah", "Oh", "Yeah"]);
        assert_eq!(starts(&pairs), times_of(&pairs, &[0, 2, 4]));

        let sung_over = song(&["Go", "Go", "Go", "Go", "Go"]);
        assert_eq!(starts(&sung_over), times_of(&sung_over, &[0, 2]));
    }

    #[test]
    fn a_song_of_one_line_repeats_it_in_pairs() {
        let lines = song(&["Around the world"; 144]);
        let every_other: Vec<usize> = (0..144).step_by(2).collect();
        assert_eq!(starts(&lines), times_of(&lines, &every_other));
    }

    #[test]
    fn breaks_and_wordless_lines_neither_belong_to_nor_split_a_chorus() {
        let lines = song(&[
            "Hold on",
            "",
            "Let go",
            "Verse here",
            "♪",
            "Hold on",
            "...",
            "Let go",
            "End",
        ]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 5]));
    }

    #[test]
    fn case_accents_and_punctuation_do_not_matter() {
        let lines = song(&[
            "Café au lait",
            "Encore une fois",
            "Middle",
            "CAFE AU LAIT!",
            "encore, une fois",
        ]);
        assert_eq!(starts(&lines), times_of(&lines, &[0, 3]));
    }

    #[test]
    fn starts_are_the_times_of_the_first_lines() {
        let lines = vec![
            LyricLine::new(-0.25, "Early start"),
            LyricLine::new(3.5, "Second line"),
            LyricLine::new(9.0, ""),
            LyricLine::new(12.75, "Something else"),
            LyricLine::new(61.2, "Early start"),
            LyricLine::new(64.0, "Second line"),
        ];
        assert_eq!(starts(&lines), [-0.25, 61.2]);
    }

    #[test]
    fn long_choruses_are_found_whole() {
        let chorus: Vec<String> = (1..=12).map(|n| format!("Chorus line {n}")).collect();
        let mut texts: Vec<String> = Vec::new();
        for verse in 1..=3 {
            texts.extend((1..=6).map(|n| format!("Verse {verse} line {n}")));
            texts.extend(chorus.iter().cloned());
        }
        let lines = song(&texts.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(starts(&lines), times_of(&lines, &[6, 24, 42]));
    }

    #[test]
    fn next_chorus_from_any_position() {
        let starts = [20.0, 60.0, 100.0];
        assert_eq!(next_after(0.0, &starts), Some(20.0), "from the intro");
        assert_eq!(next_after(20.0, &starts), Some(60.0), "right after a jump");
        assert_eq!(next_after(17.0, &starts), Some(20.0), "the coming one");
        assert_eq!(next_after(59.4, &starts), Some(60.0));
        assert_eq!(
            next_after(59.5, &starts),
            Some(100.0),
            "half a second ahead is not more than half a second"
        );
        assert_eq!(next_after(101.0, &starts), Some(20.0), "wraps to the first");
        assert_eq!(next_after(0.0, &[]), None);
    }
}
