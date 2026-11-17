//! The checks from the Swift app (`Checks/main.swift` on `preview/lyrics-overlay-0.2`),
//! ported one to one so the Rust core keeps the exact behavior listeners know.
//! The `TrackLyricsStore` checks live with the store in `lyrics-providers`.

use lyrics_core::lrc;
use lyrics_core::matching::{self, LyricsRecord};
use lyrics_core::{LyricLine, PlaybackClock};

fn times(lines: &[LyricLine]) -> Vec<f64> {
    lines.iter().map(|line| line.time).collect()
}

fn texts(lines: &[LyricLine]) -> Vec<&str> {
    lines.iter().map(|line| line.text.as_str()).collect()
}

/// Applies an edit the way `NSString.replacingCharacters(in:with:)` does, with
/// positions in UTF-16 code units.
fn apply(draft: &str, edit: &lrc::TimestampEdit) -> String {
    let units: Vec<u16> = draft.encode_utf16().collect();
    let mut result: Vec<u16> = units[..edit.start].to_vec();
    result.extend(edit.replacement.encode_utf16());
    result.extend_from_slice(&units[edit.start + edit.length..]);
    String::from_utf16(&result).expect("edits keep valid UTF-16")
}

fn utf16_index_of(haystack: &str, needle: &str) -> usize {
    let byte = haystack.find(needle).expect("needle is present");
    haystack[..byte].encode_utf16().count()
}

#[test]
fn parses_sorts_and_skips_invalid_timestamps() {
    let lines = lrc::parse("[00:03.50] Second\n[00:01.00] First\n[00:07.00][00:09.10] Again\n[00:12.0] \n[00:65.00] Invalid");
    assert_eq!(times(&lines), [1.0, 3.5, 7.0, 9.1, 12.0]);
    assert_eq!(texts(&lines), ["First", "Second", "Again", "Again", ""]);
    assert_eq!(lrc::current_index(&lines, 0.99), None);
    assert_eq!(lrc::current_index(&lines, 1.0), Some(0));
    assert_eq!(lrc::current_index(&lines, 8.0), Some(2));
    assert_eq!(lrc::current_index(&lines, 12.0), Some(4));
}

#[test]
fn imports_offsets_repeats_enhanced_tags_and_breaks() {
    let imported = "\u{FEFF}[ar:Example]\r\n[00:00.25] First\r\n[00:01.00] [00:03.00] <00:01.00>Second <00:01.50>line\r\n[00:02.00]\r\n [OFFSET: +500] ";
    let shifted = lrc::parse(imported);
    assert_eq!(
        times(&shifted),
        [-0.25, 0.5, 1.5, 2.5],
        "LRC offset and repeated timestamps"
    );
    assert_eq!(
        texts(&shifted),
        ["First", "Second line", "", "Second line"],
        "Enhanced tags and instrumental marker"
    );
    assert_eq!(
        lrc::current_index(&shifted, 1.5),
        Some(2),
        "Blank timestamp takes effect as soon as it is reached"
    );
    assert_eq!(
        lrc::current_index(&shifted, 0.0),
        Some(0),
        "A positive LRC offset can advance a line into pre-roll"
    );
    assert_eq!(
        lrc::parse("[offset:-100]\n[00:00.00] Later")
            .first()
            .map(|l| l.time),
        Some(0.1)
    );
    assert_eq!(
        lrc::parse("[00:01] Mention [00:04] here")
            .first()
            .map(|l| l.text.as_str()),
        Some("Mention [00:04] here")
    );
    assert_eq!(
        lrc::current_index(&lrc::parse("[00:01] First\n[00:01] Second"), 1.0),
        Some(1)
    );
    assert_eq!(lrc::current_index(&[], 0.0), None);
}

#[test]
fn stamps_the_line_at_the_caret() {
    let draft = "[offset:500]\r\nMorgenlicht 🌙\r\nNächste Zeile";
    let first_row_end =
        utf16_index_of(draft, "Morgenlicht 🌙") + "Morgenlicht 🌙".encode_utf16().count();
    let stamp =
        lrc::timestamp_edit(draft, first_row_end, 59.999).expect("caret is on a lyric line");
    let edited = apply(draft, &stamp);
    assert_eq!(
        edited,
        "[offset:500]\r\n[01:00.50]Morgenlicht 🌙\r\nNächste Zeile"
    );
    assert_eq!(
        lrc::parse(&edited).first().map(|l| l.time),
        Some(60.0),
        "Stamped time includes the LRC offset"
    );
    assert_eq!(stamp.next_caret, utf16_index_of(&edited, "Nächste Zeile"));
    assert_eq!(
        lrc::timestamp_edit(draft, 0, 3.0),
        None,
        "Metadata is left intact"
    );
    assert_eq!(lrc::timestamp_edit(draft, 0, f64::NAN), None);

    let repeat_draft = "  [00:02][00:08]Wieder";
    let repeated = lrc::timestamp_edit(repeat_draft, repeat_draft.encode_utf16().count(), 3.25)
        .expect("caret is on a lyric line");
    let corrected = apply(repeat_draft, &repeated);
    assert_eq!(corrected, "  [00:03.25][00:08]Wieder");
    assert_eq!(repeated.next_caret, corrected.encode_utf16().count());
    assert_eq!(
        lrc::timestamp_edit("", 0, 0.0).map(|edit| edit.replacement),
        Some("[00:00.00]".to_owned())
    );
    let after_newline = lrc::timestamp_edit("A\n", 2, 3.0).expect("empty last line");
    assert_eq!((after_newline.start, after_newline.length), (2, 0));
}

#[test]
fn playback_clock_extrapolates_between_readings() {
    let playing = PlaybackClock::new(10.0, 100.0, true);
    assert_eq!(playing.position_at(100.25, 20.0), 10.25);
    assert_eq!(
        playing.position_at(99.0, 20.0),
        10.0,
        "An earlier reading does not rewind the anchor"
    );
    assert_eq!(
        playing.position_at(115.0, 20.0),
        20.0,
        "The estimate stops at the track duration"
    );
    assert_eq!(
        playing.position_at(105.0, 0.0),
        15.0,
        "Unknown duration does not cap playback"
    );
    let paused = PlaybackClock::new(8.0, 101.0, false);
    assert_eq!(
        paused.position_at(105.0, 20.0),
        8.0,
        "Pausing or seeking uses the new snapshot"
    );
    let invalid = PlaybackClock::new(f64::NAN, f64::INFINITY, true);
    assert_eq!(invalid.position_at(10.0, f64::NAN), 0.0);
}

#[test]
fn matching_is_conservative() {
    let json = r#"[
      {"trackName":"Song","artistName":"Artist","duration":200,"syncedLyrics":null},
      {"trackName":"Song","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00]"},
      {"trackName":"Song","artistName":"Someone else","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
      {"trackName":"Song (Live)","artistName":"Artist","duration":200,"syncedLyrics":"[00:01.00] Wrong"},
      {"trackName":"Song","artistName":"Artist","duration":250,"syncedLyrics":"[00:01.00] Wrong"},
      {"trackName":"Song","artistName":"Artist","duration":201,"syncedLyrics":"[00:01.00] Right"}
    ]"#;
    let results: Vec<LyricsRecord> = serde_json::from_str(json).expect("valid LRCLIB response");
    assert!(matching::matches(&results[0], "Song", "Artist", 200.0));
    assert!(!matching::matches(&results[2], "Song", "Artist", 200.0));
    assert_eq!(
        matching::best(&results, "SONG", "Artist", 200.0).and_then(|r| r.synced_lyrics.as_deref()),
        Some("[00:01.00] Right")
    );
    assert_eq!(matching::best(&results, "Other", "Artist", 200.0), None);
}
