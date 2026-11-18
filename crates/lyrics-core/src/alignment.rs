//! Lining up the words someone sang with the lyrics of a song.
//!
//! Sing to find hears a few words through the microphone, as text with a time
//! for every word. [`align`] finds where those words stand in a song's lyrics,
//! tolerant to misheard, missing and extra words, and tells how far the lyric
//! time is ahead of the audio clock. [`rank`] orders candidate songs by how
//! well they explain what was heard, and [`Follower`] keeps the lyrics with the
//! singer while they go on, without jumping on a single doubtful match.

use std::cmp::Ordering;
use std::ops::Range;

use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::model::LyricLine;

/// Two words count as the same above this similarity, so "walkin" matches
/// "walking" but "rain" does not match "pain".
pub const SIMILAR: f64 = 0.76;
/// The least share of the heard words a match has to explain.
pub const MIN_CONFIDENCE: f64 = 0.5;
/// The least number of words a match needs.
pub const MIN_WORDS: usize = 3;

const MATCH_REWARD: f64 = 2.0;
const SUBSTITUTION: f64 = -0.6;
/// A heard word that is not in the lyrics, such as a filler or a mishearing.
const EXTRA_WORD: f64 = -0.5;
/// A lyric word that was not heard: singers skip words, recognition drops some.
const SKIPPED_WORD: f64 = -0.4;
/// Seconds a word takes when the lyrics only time whole lines.
const SECONDS_PER_WORD: f64 = 0.5;

/// A word heard through the microphone; `time` is seconds on the audio clock.
#[derive(Debug, Clone, PartialEq)]
pub struct HeardWord {
    pub text: String,
    pub time: f64,
}

impl HeardWord {
    pub fn new(text: impl Into<String>, time: f64) -> Self {
        Self {
            text: text.into(),
            time,
        }
    }
}

/// A word of the lyrics, folded for comparison, with the time it is sung.
#[derive(Debug, Clone, PartialEq)]
pub struct SongWord {
    pub text: String,
    /// Seconds from the start of the track.
    pub time: f64,
    /// Index of its line in the lyrics.
    pub line: usize,
    /// The lyrics give this word's time: it has word timing or starts its line.
    /// Other times are estimated from the length of the line.
    pub exact: bool,
}

/// Where a run of heard words stands in a song.
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// The local alignment score; higher explains more of what was heard.
    pub score: f64,
    /// Heard words that matched a lyric word.
    pub matched: usize,
    /// The share of the heard words, by weight, that matched (0 to 1).
    pub confidence: f64,
    /// The matched stretch of [`SongWord`]s.
    pub song: Range<usize>,
    /// The matched stretch of [`HeardWord`]s.
    pub heard: Range<usize>,
    /// Lyric time minus audio time: the track position is the audio clock plus this.
    pub offset: f64,
    /// The line of the last matched lyric word.
    pub line: usize,
}

impl Alignment {
    /// Enough words, and enough of what was heard, to trust this match.
    pub fn is_confident(&self) -> bool {
        self.matched >= MIN_WORDS && self.confidence >= MIN_CONFIDENCE
    }
}

/// Folds `text` into comparable words: no case, accents or punctuation, so
/// "Don't stop—believin'!" becomes `["dont", "stop", "believin"]`.
pub fn words(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || is_separator(c))
        .map(fold)
        .filter(|word| !word.is_empty())
        .collect()
}

fn is_separator(c: char) -> bool {
    matches!(c, '-' | '–' | '—' | '/' | '_' | '…' | '·' | ',' | ';' | ':')
}

fn fold(word: &str) -> String {
    word.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Every lyric word with the time it is sung: from word timing where the lyrics
/// have it, otherwise spread over the line by length.
pub fn song_words(lines: &[LyricLine]) -> Vec<SongWord> {
    let mut result = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.is_break() {
            continue;
        }
        let start = result.len();
        if line.words.is_empty() {
            timed_by_line(lines, index, &mut result);
        } else {
            timed_by_word(line, index, &mut result);
        }
        if let Some(first) = result.get_mut(start) {
            first.exact = true;
        }
    }
    result
}

fn timed_by_word(line: &LyricLine, index: usize, into: &mut Vec<SongWord>) {
    // Syllables join into words where no whitespace separates them.
    let mut text = String::new();
    let mut time: Option<f64> = None;
    let mut flush = |text: &mut String, time: &mut Option<f64>| {
        for word in words(text) {
            into.push(SongWord {
                text: word,
                time: time.unwrap_or(line.time),
                line: index,
                exact: true,
            });
        }
        text.clear();
        *time = None;
    };
    for part in &line.words {
        for c in part.text.chars() {
            if c.is_whitespace() {
                flush(&mut text, &mut time);
            } else {
                if text.is_empty() {
                    time = Some(part.start);
                }
                text.push(c);
            }
        }
    }
    flush(&mut text, &mut time);
}

fn timed_by_line(lines: &[LyricLine], index: usize, into: &mut Vec<SongWord>) {
    let line = &lines[index];
    let found = words(&line.text);
    if found.is_empty() {
        return;
    }
    let next = lines.get(index + 1).map(|next| next.time);
    let natural = SECONDS_PER_WORD * found.len() as f64;
    let span = match line.end.or(next) {
        Some(end) if end > line.time => (end - line.time).min(natural.max(1.0)),
        _ => natural,
    };
    let letters: usize = found.iter().map(|word| word.chars().count()).sum();
    let mut before = 0;
    for word in found {
        let time = line.time + span * before as f64 / letters.max(1) as f64;
        before += word.chars().count();
        into.push(SongWord {
            text: word,
            time,
            line: index,
            exact: false,
        });
    }
}

/// How alike two folded words are, from 0 to 1. Words of one or two letters
/// have to be equal.
pub fn similarity(left: &str, right: &str) -> f64 {
    if left == right {
        return 1.0;
    }
    let a: Vec<char> = left.chars().collect();
    let b: Vec<char> = right.chars().collect();
    if a.len() <= 2 || b.len() <= 2 {
        return 0.0;
    }
    let longest = a.len().max(b.len());
    1.0 - levenshtein(&a, &b) as f64 / longest as f64
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(ca != cb);
            current[j + 1] = substitution.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

/// Short words like "a" or "the" say little about which song it is.
fn weight(word: &str) -> f64 {
    (word.chars().count().min(6) as f64 / 6.0).max(0.34)
}

#[derive(Clone, Copy, PartialEq)]
enum Step {
    Start,
    Match,
    Substitute,
    ExtraWord,
    SkippedWord,
}

/// The best place for `heard` anywhere in `song`, or `None` when nothing matches.
pub fn align(heard: &[HeardWord], song: &[SongWord]) -> Option<Alignment> {
    align_within(heard, song, 0..song.len())
}

/// The best place for `heard` inside `range` of `song`. Smith-Waterman local
/// alignment over words, with fuzzy word equality.
pub fn align_within(
    heard: &[HeardWord],
    song: &[SongWord],
    range: Range<usize>,
) -> Option<Alignment> {
    let range = range.start.min(song.len())..range.end.min(song.len());
    let lyrics = &song[range.clone()];
    if heard.is_empty() || lyrics.is_empty() {
        return None;
    }
    let folded: Vec<String> = heard.iter().map(|word| fold(&word.text)).collect();
    let weights: Vec<f64> = folded.iter().map(|word| weight(word)).collect();
    let (rows, columns) = (heard.len() + 1, lyrics.len() + 1);
    let mut scores = vec![0.0_f64; rows * columns];
    let mut steps = vec![Step::Start; rows * columns];
    let mut best = (0.0, 0, 0);
    for i in 1..rows {
        for j in 1..columns {
            let alike = similarity(&folded[i - 1], &lyrics[j - 1].text);
            let diagonal = scores[(i - 1) * columns + j - 1];
            let (diagonal, diagonal_step) = if alike >= SIMILAR && !folded[i - 1].is_empty() {
                (
                    diagonal + MATCH_REWARD * weights[i - 1] * alike,
                    Step::Match,
                )
            } else {
                (diagonal + SUBSTITUTION, Step::Substitute)
            };
            let candidates = [
                (0.0, Step::Start),
                (diagonal, diagonal_step),
                (scores[(i - 1) * columns + j] + EXTRA_WORD, Step::ExtraWord),
                (
                    scores[i * columns + j - 1] + SKIPPED_WORD,
                    Step::SkippedWord,
                ),
            ];
            let (score, step) = candidates
                .into_iter()
                .fold((0.0, Step::Start), |best, next| {
                    if next.0 > best.0 {
                        next
                    } else {
                        best
                    }
                });
            scores[i * columns + j] = score;
            steps[i * columns + j] = step;
            // Strictly greater: on a tie the earlier place in the song wins.
            if score > best.0 {
                best = (score, i, j);
            }
        }
    }
    let (score, mut i, mut j) = best;
    if score <= 0.0 {
        return None;
    }
    let (end_heard, end_song) = (i, j);
    let mut pairs = Vec::new();
    loop {
        match steps[i * columns + j] {
            Step::Start => break,
            Step::Match => {
                pairs.push((i - 1, j - 1));
                i -= 1;
                j -= 1;
            }
            Step::Substitute => {
                i -= 1;
                j -= 1;
            }
            Step::ExtraWord => i -= 1,
            Step::SkippedWord => j -= 1,
        }
        if i == 0 || j == 0 {
            break;
        }
    }
    pairs.reverse();
    let (&(first_heard, first_song), &(_, last_song)) = (pairs.first()?, pairs.last()?);
    let matched_weight: f64 = pairs.iter().map(|&(h, _)| weights[h]).sum();
    let total_weight: f64 = weights.iter().sum();
    // Words with an exact lyric time tell the offset best; estimated ones only
    // when no exact word was heard.
    let exact: Vec<f64> = pairs
        .iter()
        .filter(|&&(_, s)| lyrics[s].exact)
        .map(|&(h, s)| lyrics[s].time - heard[h].time)
        .collect();
    let differences = if exact.is_empty() {
        pairs
            .iter()
            .map(|&(h, s)| lyrics[s].time - heard[h].time)
            .collect()
    } else {
        exact
    };
    Some(Alignment {
        score,
        matched: pairs.len(),
        confidence: (matched_weight / total_weight).clamp(0.0, 1.0),
        song: range.start + first_song..range.start + end_song.max(last_song + 1),
        heard: first_heard..end_heard,
        offset: median(differences),
        line: lyrics[last_song].line,
    })
}

/// The middle value, or the mean of the two middle ones.
fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    match values.len() {
        0 => 0.0,
        n if n % 2 == 1 => values[n / 2],
        n => (values[n / 2 - 1] + values[n / 2]) / 2.0,
    }
}

/// The song words sung between `from` and `to` seconds into the track.
pub fn words_between(song: &[SongWord], from: f64, to: f64) -> Range<usize> {
    let start = song.partition_point(|word| word.time < from);
    let end = song.partition_point(|word| word.time <= to);
    start..end.max(start)
}

/// Orders candidate songs by how well each explains `heard`: the best first,
/// ties in the order given. Songs where nothing matches are left out.
pub fn rank(heard: &[HeardWord], songs: &[Vec<SongWord>]) -> Vec<(usize, Alignment)> {
    let mut ranked: Vec<(usize, Alignment)> = songs
        .iter()
        .enumerate()
        .filter_map(|(index, song)| align(heard, song).map(|alignment| (index, alignment)))
        .collect();
    ranked.sort_by(|(a_index, a), (b_index, b)| {
        b.is_confident()
            .cmp(&a.is_confident())
            .then_with(|| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal))
            .then_with(|| a_index.cmp(b_index))
    });
    ranked
}

/// How a [`Follower`] took a new alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FollowStep {
    /// Too doubtful, or it agrees with the current timing.
    Kept,
    /// A small correction, applied in part to stay smooth.
    Nudged { by: f64 },
    /// A large correction, applied because two alignments agreed on it.
    Jumped { by: f64 },
    /// A large correction that waits for a second alignment to confirm it.
    Waiting,
}

/// Keeps the lyrics with someone who goes on singing. Small drifts are
/// corrected at once; a large jump, such as a misheard line that also fits a
/// repeated chorus, only after two alignments in a row agree on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Follower {
    offset: f64,
    pending: Option<f64>,
}

impl Follower {
    /// Shifts smaller than this are noise and change nothing.
    pub const STEADY: f64 = 0.25;
    /// Shifts up to this are corrected at once.
    pub const NUDGE: f64 = 2.0;
    /// Two large shifts confirm each other within this many seconds.
    pub const AGREE: f64 = 1.5;

    /// Starts with lyric time = audio time + `offset`.
    pub fn new(offset: f64) -> Self {
        Self {
            offset,
            pending: None,
        }
    }

    /// Lyric time minus audio time.
    pub fn offset(&self) -> f64 {
        self.offset
    }

    /// The lyric time at `audio_time`.
    pub fn position_at(&self, audio_time: f64) -> f64 {
        audio_time + self.offset
    }

    /// Takes the alignment of the latest heard words.
    pub fn observe(&mut self, alignment: &Alignment) -> FollowStep {
        if !alignment.is_confident() {
            return FollowStep::Kept;
        }
        let shift = alignment.offset - self.offset;
        if shift.abs() <= Self::STEADY {
            self.pending = None;
            return FollowStep::Kept;
        }
        if shift.abs() <= Self::NUDGE {
            self.pending = None;
            let by = shift * 0.6;
            self.offset += by;
            return FollowStep::Nudged { by };
        }
        match self.pending.take() {
            Some(previous) if (previous - alignment.offset).abs() <= Self::AGREE => {
                let target = (previous + alignment.offset) / 2.0;
                let by = target - self.offset;
                self.offset = target;
                FollowStep::Jumped { by }
            }
            _ => {
                self.pending = Some(alignment.offset);
                FollowStep::Waiting
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lrc;

    const SONG: &str = "\
[00:04.00]The river carries every name we gave the summer
[00:08.00]Paper boats are sailing past the silent harbor
[00:12.00]I can hear the morning calling from the mountain
[00:16.00]Every window opens when the swallows come home
[00:20.00]Hold the lantern higher so the travelers find us
[00:24.00]
[00:30.00]I can hear the morning calling from the mountain
[00:34.00]We will sing until the valley answers back
";

    fn song() -> Vec<SongWord> {
        song_words(&lrc::parse(SONG))
    }

    /// Words as if sung from `start` seconds on the audio clock, one every 0.3 s.
    fn sung(text: &str, start: f64) -> Vec<HeardWord> {
        words(text)
            .into_iter()
            .enumerate()
            .map(|(index, word)| HeardWord::new(word, start + 0.3 * index as f64))
            .collect()
    }

    #[test]
    fn folds_case_accents_and_punctuation() {
        assert_eq!(words("Don't stop—believin'!"), ["dont", "stop", "believin"]);
        assert_eq!(words("  Ça VA, très bien… "), ["ca", "va", "tres", "bien"]);
        assert_eq!(words("Über Dächern, sacht"), ["uber", "dachern", "sacht"]);
        assert!(words(" ♪ … ").is_empty());
    }

    #[test]
    fn similarity_tolerates_small_mishearings() {
        assert_eq!(similarity("morning", "morning"), 1.0);
        assert!(similarity("walkin", "walking") >= SIMILAR);
        assert!(similarity("lanterns", "lantern") >= SIMILAR);
        assert!(similarity("rain", "pain") < SIMILAR);
        assert_eq!(similarity("in", "on"), 0.0, "short words must be equal");
    }

    #[test]
    fn line_timing_spreads_words_and_marks_line_starts() {
        let words = song();
        let first = &words[0];
        assert_eq!(
            (first.text.as_str(), first.time, first.exact),
            ("the", 4.0, true)
        );
        let second = &words[1];
        assert!(second.time > 4.0 && second.time < 5.0 && !second.exact);
        assert!(words.windows(2).all(|pair| pair[0].time <= pair[1].time));
        // The break line adds no words.
        assert!(words.iter().all(|word| word.line != 5));
    }

    #[test]
    fn word_timing_joins_syllables_into_words() {
        let lines = lrc::parse(
            "[00:00.00]<00:00.00>Leise <00:00.70>zieht <00:02.40>vor<00:02.90>bei<00:03.60>\n",
        );
        let words = song_words(&lines);
        let texts: Vec<_> = words
            .iter()
            .map(|word| (word.text.as_str(), word.time))
            .collect();
        assert_eq!(texts, [("leise", 0.0), ("zieht", 0.7), ("vorbei", 2.4)]);
        assert!(words.iter().all(|word| word.exact));
    }

    #[test]
    fn finds_the_sung_line_and_the_offset() {
        // The third line, sung starting at 100 s on the audio clock.
        let heard = sung("i can hear the morning calling from the mountain", 100.0);
        let alignment = align(&heard, &song()).expect("matches");
        assert!(alignment.is_confident());
        assert_eq!(alignment.matched, 9);
        assert!(alignment.confidence > 0.99);
        assert_eq!(alignment.line, 2, "the first of the two equal lines");
        assert!(
            (alignment.offset - (12.0 - 100.0)).abs() < 0.01,
            "{}",
            alignment.offset
        );
    }

    #[test]
    fn tolerates_misheard_missing_and_extra_words() {
        let heard = sung("oh paper boat are selling past uh the silent harbour", 50.0);
        let alignment = align(&heard, &song()).expect("matches");
        assert!(alignment.is_confident(), "{alignment:?}");
        assert_eq!(alignment.line, 1);
        assert!(
            (alignment.offset - (8.0 - 50.3)).abs() < 0.01,
            "{}",
            alignment.offset
        );
    }

    #[test]
    fn spans_two_lines() {
        let heard = sung("the swallows come home hold the lantern higher", 10.0);
        let alignment = align(&heard, &song()).expect("matches");
        assert!(alignment.is_confident());
        assert_eq!(alignment.line, 4, "ends in the fifth line");
    }

    #[test]
    fn unrelated_words_are_not_confident() {
        let heard = sung("never gonna give you up never gonna let you down", 0.0);
        assert!(align(&heard, &song()).is_none_or(|alignment| !alignment.is_confident()));
        assert!(align(&[], &song()).is_none());
        assert!(align(&heard, &[]).is_none());
    }

    #[test]
    fn a_range_picks_the_repeated_line_nearby() {
        let heard = sung("i can hear the morning calling from the mountain", 0.0);
        let words = song();
        let late = words_between(&words, 26.0, 40.0);
        let alignment = align_within(&heard, &words, late).expect("matches");
        assert_eq!(alignment.line, 6, "the repetition after the break");
        assert!((alignment.offset - 30.0).abs() < 0.01);
    }

    #[test]
    fn ranks_the_song_that_explains_the_words_first() {
        let decoy = song_words(&lrc::parse(
            "[00:01.00]Good morning to the city\n[00:05.00]The mountain is calling me\n",
        ));
        let right = song();
        let heard = sung("i can hear the morning calling from the mountain", 3.0);
        let ranked = rank(&heard, &[decoy.clone(), right, Vec::new()]);
        assert_eq!(ranked.len(), 2, "the empty song is left out");
        assert_eq!(ranked[0].0, 1);
        assert!(ranked[0].1.is_confident());
        assert!(!ranked[1].1.is_confident());

        // Equal songs keep the order given.
        let twice = rank(&heard, &[song(), song()]);
        assert_eq!(
            twice.iter().map(|(index, _)| *index).collect::<Vec<_>>(),
            [0, 1]
        );
        assert!(rank(&heard, &[decoy])
            .iter()
            .all(|(_, a)| !a.is_confident()));
    }

    fn alignment(offset: f64, matched: usize) -> Alignment {
        Alignment {
            score: 10.0,
            matched,
            confidence: 0.9,
            song: 0..matched,
            heard: 0..matched,
            offset,
            line: 0,
        }
    }

    #[test]
    fn the_follower_nudges_small_drifts_at_once() {
        let mut follower = Follower::new(10.0);
        assert_eq!(follower.observe(&alignment(10.1, 5)), FollowStep::Kept);
        assert_eq!(follower.offset(), 10.0);
        let FollowStep::Nudged { by } = follower.observe(&alignment(11.0, 5)) else {
            panic!("expected a nudge");
        };
        assert!((by - 0.6).abs() < 1e-9);
        assert!((follower.position_at(2.0) - 12.6).abs() < 1e-9);
        assert_eq!(
            follower.observe(&alignment(30.0, 2)),
            FollowStep::Kept,
            "too few words"
        );
    }

    #[test]
    fn the_follower_jumps_only_when_two_alignments_agree() {
        let mut follower = Follower::new(0.0);
        assert_eq!(follower.observe(&alignment(-18.0, 6)), FollowStep::Waiting);
        assert_eq!(follower.offset(), 0.0, "one doubtful match moves nothing");
        assert_eq!(
            follower.observe(&alignment(25.0, 6)),
            FollowStep::Waiting,
            "disagrees"
        );
        let step = follower.observe(&alignment(24.0, 6));
        assert_eq!(step, FollowStep::Jumped { by: 24.5 });
        assert_eq!(follower.offset(), 24.5);
        // A steady reading clears a waiting jump.
        assert_eq!(follower.observe(&alignment(40.0, 6)), FollowStep::Waiting);
        assert_eq!(follower.observe(&alignment(24.6, 6)), FollowStep::Kept);
        assert_eq!(follower.observe(&alignment(40.5, 6)), FollowStep::Waiting);
    }

    #[test]
    fn follows_a_singer_through_the_song() {
        let words = song();
        // Sung 2 s later than the lyrics say: the audio clock runs at lyric time − 2.
        let first = align(
            &sung("paper boats are sailing past the silent harbor", 6.0),
            &words,
        )
        .expect("matches");
        let mut follower = Follower::new(first.offset);
        assert!((follower.offset() - 2.0).abs() < 0.01);
        // Later the singer drags by another second.
        let heard = sung("hold the lantern higher so the travelers find us", 17.0);
        let expected = follower.position_at(heard[0].time);
        let near = words_between(&words, expected - 8.0, expected + 12.0);
        let next = align_within(&heard, &words, near).expect("matches");
        assert_eq!(next.line, 4);
        assert!(matches!(follower.observe(&next), FollowStep::Nudged { .. }));
        assert!(
            (follower.offset() - 2.6).abs() < 0.01,
            "{}",
            follower.offset()
        );
    }

    #[test]
    fn without_a_line_start_the_estimated_times_tell_the_offset() {
        // From the middle of the third line on, at lyric time − 1.
        let heard: Vec<HeardWord> = ["morning", "calling", "from", "the", "mountain"]
            .iter()
            .zip([12.1, 12.8, 13.5, 13.9, 14.2])
            .map(|(word, time)| HeardWord::new(*word, time))
            .collect();
        let alignment = align(&heard, &song()).expect("matches");
        assert!(alignment.is_confident());
        assert!((alignment.offset - 1.0).abs() < 0.1, "{}", alignment.offset);
    }
}
