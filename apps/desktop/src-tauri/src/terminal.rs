//! `singline lyrics`: the lyrics in the terminal, for coding with music. It
//! reads `now.json` like `singline now` and fills the terminal: the song at the
//! top, the line being sung in the middle, lighting up word by word, the lines
//! around it dimmer, and the time at the bottom. q, Esc or Ctrl+C leaves.

use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{cursor, execute, queue};
use lyrics_core::{LyricLine, SyncLevel};

use crate::now::{self, Now};

/// How often the view checks the file and the clock.
const TICK: Duration = Duration::from_millis(80);

const HELP: &str = "Usage: singline lyrics

Shows the lyrics of the song Singline plays in this terminal, the line being
sung lighting up word by word. Singline must be running. q, Esc or Ctrl+C
leaves.";

/// How a piece of text looks. The terminal keeps Singline's monochrome look:
/// bright for what is sung, dimmer the further away a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Song,
    Sung,
    Coming,
    Near,
    Far,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub look: Look,
}

/// One screen: exactly one row of pieces per terminal row.
pub type Frame = Vec<Vec<Piece>>;

fn piece(text: impl Into<String>, look: Look) -> Piece {
    Piece {
        text: text.into(),
        look,
    }
}

/// Shortens text to `max` characters, ending with an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

/// Centres the pieces of one row in `width` columns.
fn centred(pieces: Vec<Piece>, width: usize) -> Vec<Piece> {
    let length: usize = pieces.iter().map(|piece| piece.text.chars().count()).sum();
    let mut row = Vec::with_capacity(pieces.len() + 1);
    if length < width {
        row.push(piece(" ".repeat((width - length) / 2), Look::Hint));
    }
    row.extend(pieces);
    row
}

fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.0) as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// Splits a line at `time` into what is sung and what is still to come: by
/// its words where they are timed, else by how far the line has come, after
/// whole words.
pub fn split(line: &LyricLine, next: Option<f64>, time: f64) -> (String, String) {
    let text = line.text.trim();
    if !line.words.is_empty() {
        let sung: String = line
            .words
            .iter()
            .filter(|word| word.start <= time)
            .map(|word| word.text.as_str())
            .collect();
        let rest: String = line
            .words
            .iter()
            .filter(|word| word.start > time)
            .map(|word| word.text.as_str())
            .collect();
        let sung = sung.trim_start().to_owned();
        let rest = if sung.trim().is_empty() {
            rest.trim().to_owned()
        } else {
            rest.trim_end().to_owned()
        };
        return (sung, rest);
    }
    let end = line.end.or(next).unwrap_or(line.time + 4.0);
    let progress = if end > line.time {
        ((time - line.time) / (end - line.time)).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let characters: Vec<char> = text.chars().collect();
    let cut = if progress >= 1.0 {
        characters.len()
    } else {
        let reached = (characters.len() as f64 * progress).round() as usize;
        characters[..(reached + 1).min(characters.len())]
            .iter()
            .rposition(|character| *character == ' ')
            .map_or(0, |space| space + 1)
    };
    (
        characters[..cut].iter().collect(),
        characters[cut..].iter().collect(),
    )
}

/// The screen for `now` at `at_ms` in a terminal of `width` × `height`.
pub fn frame(now: Option<&Now>, at_ms: u64, width: u16, height: u16) -> Frame {
    let (width, height) = (usize::from(width), usize::from(height));
    let mut rows: Frame = vec![Vec::new(); height];
    if height == 0 || width < 8 {
        return rows;
    }
    let message = |rows: &mut Frame, text: &str| {
        rows[height / 2] = centred(vec![piece(shorten(text, width - 2), Look::Hint)], width);
    };
    let Some(now) = now.filter(|now| now::is_fresh(now, at_ms)) else {
        message(&mut rows, "Singline is not running.");
        return rows;
    };
    let Some(title) = now.title.as_deref() else {
        message(&mut rows, "Play a song, and its lyrics show here.");
        return rows;
    };
    let artist = now.artist.as_deref().unwrap_or("");
    let song = if artist.is_empty() {
        format!("♪ {title}")
    } else {
        format!("♪ {title} – {artist}")
    };
    rows[0] = centred(vec![piece(shorten(&song, width - 2), Look::Song)], width);
    let position = now::position_at(now, at_ms);
    if height >= 6 {
        if let (Some(position), Some(duration)) = (position, now.duration.filter(|d| *d > 0.0)) {
            rows[height - 1] = centred(time_bar(position, duration, width), width);
        }
    }

    let area = if height >= 6 {
        2..height - 2
    } else {
        1..height
    };
    let middle = area.start + (area.len().saturating_sub(1)) / 2;
    let Some(lyrics) = now.lyrics.as_ref() else {
        message(&mut rows, "Looking for the lyrics…");
        return rows;
    };
    if lyrics.instrumental {
        rows[middle] = centred(vec![piece("Instrumental", Look::Hint)], width);
        return rows;
    }
    let (Some(position), true) = (position, lyrics.sync != SyncLevel::Unsynced) else {
        rows[middle] = centred(
            vec![piece("These lyrics have no timing.", Look::Hint)],
            width,
        );
        return rows;
    };
    let time = position + now.offset;
    let lines = &lyrics.lines;
    let current = lyrics_core::lrc::current_index(lines, time);
    // Before the first line, the lines to come wait below the middle.
    let anchor = current.map_or(-1, |index| index as isize);
    for row in area {
        let index = anchor + row as isize - middle as isize;
        let Some(line) = usize::try_from(index)
            .ok()
            .and_then(|index| lines.get(index))
        else {
            continue;
        };
        let distance = (index - anchor).unsigned_abs();
        let room = width - 4;
        let pieces = if line.text.trim().is_empty() {
            let look = if distance == 0 {
                Look::Coming
            } else {
                Look::Far
            };
            vec![piece("·  ·  ·", look)]
        } else if Some(index as usize) == current {
            let next = lines.get(index as usize + 1).map(|line| line.time);
            let (sung, rest) = split(line, next, time);
            let sung = shorten(&sung, room);
            let rest = shorten(&rest, room.saturating_sub(sung.chars().count()));
            vec![piece(sung, Look::Sung), piece(rest, Look::Coming)]
        } else {
            let look = if distance == 1 { Look::Near } else { Look::Far };
            vec![piece(shorten(line.text.trim(), room), look)]
        };
        rows[row] = centred(pieces, width);
    }
    rows
}

/// "0:35 ━━━━━━──────────── −2:36", as wide as the terminal allows.
fn time_bar(position: f64, duration: f64, width: usize) -> Vec<Piece> {
    let elapsed = clock(position);
    let remaining = format!("−{}", clock(duration - position));
    let bar = width
        .saturating_sub(elapsed.chars().count() + remaining.chars().count() + 6)
        .min(48);
    let done = ((position / duration).clamp(0.0, 1.0) * bar as f64).round() as usize;
    vec![
        piece(format!("{elapsed}  "), Look::Hint),
        piece("━".repeat(done), Look::Near),
        piece("─".repeat(bar - done), Look::Far),
        piece(format!("  {remaining}"), Look::Hint),
    ]
}

/// Puts the terminal back as it was, also when the view panics.
struct Screen;

impl Screen {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

fn colour(look: Look) -> (Color, bool) {
    match look {
        Look::Sung => (Color::White, true),
        Look::Coming => (Color::AnsiValue(250), false),
        Look::Song => (Color::AnsiValue(248), false),
        Look::Near => (Color::AnsiValue(244), false),
        Look::Far | Look::Hint => (Color::AnsiValue(240), false),
    }
}

fn draw(out: &mut impl Write, frame: &Frame) -> io::Result<()> {
    queue!(out, BeginSynchronizedUpdate)?;
    for (row, pieces) in frame.iter().enumerate() {
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        queue!(
            out,
            cursor::MoveTo(0, row),
            terminal::Clear(terminal::ClearType::CurrentLine)
        )?;
        for piece in pieces {
            let (colour, bold) = colour(piece.look);
            if bold {
                queue!(out, SetAttribute(Attribute::Bold))?;
            }
            queue!(
                out,
                SetForegroundColor(colour),
                Print(&piece.text),
                SetAttribute(Attribute::Reset)
            )?;
        }
    }
    queue!(out, EndSynchronizedUpdate)?;
    out.flush()
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// `singline lyrics`: runs the view until the listener leaves. Returns the exit code.
pub fn run(arguments: &[String]) -> i32 {
    match arguments.first().map(String::as_str) {
        None => {}
        Some("--help" | "-h") => {
            println!("{HELP}");
            return 0;
        }
        Some(other) => {
            eprintln!("Unknown option {other}.\n\n{HELP}");
            return 2;
        }
    }
    match show() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("singline lyrics: {error}");
            1
        }
    }
}

fn show() -> io::Result<()> {
    let _screen = Screen::enter()?;
    let mut out = io::stdout();
    let mut reader = now::Reader::default();
    let mut shown: Option<Frame> = None;
    loop {
        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let control_c = key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL);
                    if control_c || matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                        return Ok(());
                    }
                }
                Event::Resize(..) => shown = None,
                _ => {}
            }
        }
        let (width, height) = terminal::size()?;
        let frame = frame(reader.read(), epoch_ms(), width, height);
        if shown.as_ref() != Some(&frame) {
            draw(&mut out, &frame)?;
            shown = Some(frame);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyrics_core::{LyricWord, Lyrics};

    fn text(row: &[Piece]) -> String {
        row.iter()
            .map(|piece| piece.text.as_str())
            .collect::<String>()
            .trim()
            .to_owned()
    }

    fn song(position: f64) -> Now {
        Now {
            version: now::VERSION,
            updated_at: 10_000,
            player: Some("Spotify".into()),
            title: Some("Lady of Namek".into()),
            artist: Some("Tory Lanez".into()),
            album: None,
            duration: Some(194.0),
            playing: false,
            position: Some(position),
            at: 10_000,
            rate: 1.0,
            offset: 0.0,
            lyrics: Some(Lyrics {
                lines: vec![
                    LyricLine {
                        words: vec![
                            LyricWord {
                                start: 10.0,
                                end: None,
                                text: "So, ".into(),
                            },
                            LyricWord {
                                start: 10.6,
                                end: None,
                                text: "can ".into(),
                            },
                            LyricWord {
                                start: 11.2,
                                end: None,
                                text: "we?".into(),
                            },
                        ],
                        ..LyricLine::new(10.0, "So, can we?")
                    },
                    LyricLine::new(12.0, "Play with the stars tonight?"),
                    LyricLine::new(15.0, ""),
                    LyricLine::new(18.0, "Can we make them all align?"),
                ],
                sync: SyncLevel::Word,
                instrumental: false,
            }),
        }
    }

    #[test]
    fn the_line_being_sung_sits_in_the_middle_lit_up_word_by_word() {
        let rows = frame(Some(&song(10.7)), 10_000, 60, 12);
        assert_eq!(text(&rows[0]), "♪ Lady of Namek – Tory Lanez");
        // Rows 2 to 9 hold the lyrics; the middle one is row 5.
        let current = &rows[5];
        assert!(current.contains(&piece("So, can ", Look::Sung)));
        assert!(current.contains(&piece("we?", Look::Coming)));
        assert_eq!(text(&rows[6]), "Play with the stars tonight?");
        assert_eq!(rows[6].last().map(|p| p.look), Some(Look::Near));
        assert_eq!(text(&rows[7]), "·  ·  ·");
        assert_eq!(text(&rows[8]), "Can we make them all align?");
        assert_eq!(rows[8].last().map(|p| p.look), Some(Look::Far));
        assert!(text(&rows[11]).starts_with("0:10"));
        assert!(text(&rows[11]).ends_with("−3:03"));
    }

    #[test]
    fn before_the_first_line_the_lyrics_wait_below_the_middle() {
        let rows = frame(Some(&song(2.0)), 10_000, 60, 12);
        assert_eq!(text(&rows[5]), "");
        assert_eq!(text(&rows[6]), "So, can we?");
    }

    #[test]
    fn says_why_there_is_nothing_to_show() {
        let rows = frame(None, 10_000, 60, 12);
        assert_eq!(text(&rows[6]), "Singline is not running.");
        let stale = frame(Some(&song(10.0)), 40_000, 60, 12);
        assert_eq!(text(&stale[6]), "Singline is not running.");
        let mut quiet = song(10.0);
        quiet.title = None;
        assert_eq!(
            text(&frame(Some(&quiet), 10_000, 60, 12)[6]),
            "Play a song, and its lyrics show here."
        );
        let mut plain = song(10.0);
        if let Some(lyrics) = plain.lyrics.as_mut() {
            lyrics.sync = SyncLevel::Unsynced;
        }
        assert_eq!(
            text(&frame(Some(&plain), 10_000, 60, 12)[5]),
            "These lyrics have no timing."
        );
    }

    #[test]
    fn long_lines_are_cut_to_the_terminal() {
        let rows = frame(Some(&song(12.5)), 10_000, 20, 12);
        assert!(rows.iter().all(|row| text(row).chars().count() <= 20));
    }

    #[test]
    fn estimates_words_for_lines_without_word_timing() {
        let line = LyricLine::new(12.0, "Play with the stars tonight?");
        assert_eq!(
            split(&line, Some(15.0), 12.0),
            (String::new(), "Play with the stars tonight?".into())
        );
        assert_eq!(
            split(&line, Some(15.0), 13.0),
            ("Play with ".into(), "the stars tonight?".into())
        );
        assert_eq!(
            split(&line, Some(15.0), 15.0),
            ("Play with the stars tonight?".into(), String::new())
        );
    }

    #[test]
    fn tiny_terminals_get_an_empty_screen() {
        assert!(frame(Some(&song(10.0)), 10_000, 4, 3)
            .iter()
            .all(Vec::is_empty));
    }
}
