//! `singline lyrics`: the lyrics in the terminal, for coding with music. It
//! reads `now.json` like `singline now` and fills the terminal: the song at the
//! top, the line being sung in the middle, lighting up as it is sung like the
//! overlay's word highlight, the lines around it dimmer, and the time at the
//! bottom. Every colour is blended from the terminal's own text and background
//! colours, so the view suits any theme, dark or light. q, Esc or Ctrl+C leaves.

use std::io::{self, Write};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Color, Print, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{cursor, execute, queue};
use lyrics_core::{LyricLine, SyncLevel};

use crate::now::{self, Now};

/// How often the view checks the file and the clock: often enough for the
/// highlight to glide along a line.
const TICK: Duration = Duration::from_millis(40);

/// How long, in seconds, a line takes to light up when its turn comes, and the
/// line before it to dim.
const FADE: f64 = 0.3;

/// How long the terminal has to tell its colours.
const COLOUR_QUERY: Duration = Duration::from_millis(250);

/// A timed word without an end lasts at most this many seconds.
const LONGEST_WORD: f64 = 1.5;
/// Seconds per character of a line without word timing, counting one space per word.
const SECONDS_PER_CHARACTER: f64 = 0.07;
/// Even a short line is sung over at least this many seconds.
const SHORTEST_LINE: f64 = 0.6;
/// A line is sung within this share of the time until the next line starts.
const SHARE_OF_GAP: f64 = 0.9;
/// Time allowed for the last line, which has no next line to end it.
const LAST_LINE_GAP: f64 = 6.0;

/// An instrumental break.
const DOTS: &str = "·  ·  ·";

const HELP: &str = "Usage: singline lyrics

Shows the lyrics of the song Singline plays in this terminal, the line being
sung lighting up as it is sung. Singline must be running. q, Esc or Ctrl+C
leaves.";

/// How bright a piece of text is, from 0, the terminal's background, to 100,
/// its text colour. The view keeps Singline's monochrome look: bright for what
/// is sung, dimmer the further away a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tone(pub u8);

impl Tone {
    /// What has been sung of the line being sung.
    pub const SUNG: Self = Self(100);
    /// The rest of the line being sung.
    pub const COMING: Self = Self(56);
    /// The song's title.
    pub const TITLE: Self = Self(72);
    /// The lines right before and after the line being sung, and the artist.
    pub const NEAR: Self = Self(42);
    /// Hints and the time.
    pub const HINT: Self = Self(36);
    /// Lines further away.
    pub const FAR: Self = Self(24);

    /// `amount` of the way from this tone to `other`, `amount` from 0 to 1.
    pub fn towards(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let level = f64::from(self.0) + (f64::from(other.0) - f64::from(self.0)) * amount;
        Self(level.round() as u8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub tone: Tone,
}

/// One screen: exactly one row of pieces per terminal row.
pub type Frame = Vec<Vec<Piece>>;

fn piece(text: impl Into<String>, tone: Tone) -> Piece {
    Piece {
        text: text.into(),
        tone,
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
        row.push(piece(" ".repeat((width - length) / 2), Tone::HINT));
    }
    row.extend(pieces);
    row
}

fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.0) as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// A word of the line being sung, and how much of it has been sung, from 0 to 1.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub text: String,
    pub sung: f64,
}

/// How far each word of `line` has been sung at `time`, like the overlay's word
/// highlight: by the words' own timing where the lyrics have it, otherwise by
/// an estimate of about 0.07 s per character, at least 0.6 s for the line and
/// at most 90 % of the time until the next line starts at `next`. The words
/// keep their spaces, so joined they give the line's text back.
pub fn fills(line: &LyricLine, next: Option<f64>, time: f64) -> Vec<Fill> {
    if !line.words.is_empty() {
        let line_end = line.end.or(next).unwrap_or(f64::INFINITY);
        return line
            .words
            .iter()
            .enumerate()
            .map(|(index, word)| {
                let following = line.words.get(index + 1).map(|word| word.start);
                let end = word
                    .end
                    .or(following)
                    .unwrap_or_else(|| line_end.min(word.start + LONGEST_WORD));
                Fill {
                    text: word.text.clone(),
                    sung: share(time - word.start, end - word.start),
                }
            })
            .collect();
    }
    let pieces = words(&line.text);
    let weights: Vec<f64> = pieces
        .iter()
        .map(|piece| (piece.trim().chars().count() + 1) as f64)
        .collect();
    let total: f64 = weights.iter().sum();
    let sung = ((end_of(line, next) - line.time).max(0.0) * SHARE_OF_GAP)
        .min((total * SECONDS_PER_CHARACTER).max(SHORTEST_LINE));
    let mut start = line.time;
    pieces
        .into_iter()
        .zip(weights)
        .map(|(text, weight)| {
            let length = sung * weight / total;
            let fill = Fill {
                text: text.to_owned(),
                sung: share(time - start, length),
            };
            start += length;
            fill
        })
        .collect()
}

/// When `line` ends: at its own end time, else when the next line starts.
fn end_of(line: &LyricLine, next: Option<f64>) -> f64 {
    line.end
        .filter(|end| *end > line.time)
        .or(next)
        .unwrap_or(line.time + LAST_LINE_GAP)
}

/// How much of `length` seconds `elapsed` seconds cover, from 0 to 1.
fn share(elapsed: f64, length: f64) -> f64 {
    (elapsed / length.max(0.05)).clamp(0.0, 1.0)
}

/// The words of `text`, each with the spaces after it, the first also with
/// those before it.
fn words(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut after_space = false;
    let mut in_text = false;
    for (index, character) in text.char_indices() {
        let space = character.is_whitespace();
        if !space && after_space && in_text {
            pieces.push(&text[start..index]);
            start = index;
        }
        in_text |= !space;
        after_space = space;
    }
    if in_text {
        pieces.push(&text[start..]);
    }
    pieces
}

/// The line being sung: what has been sung at full brightness, the rest in
/// `rest`, and the letter being sung in between, so the highlight glides along
/// the line instead of jumping from word to word.
fn sung_line(fills: &[Fill], rest: Tone, room: usize) -> Vec<Piece> {
    let mut characters: Vec<(char, Tone)> = Vec::new();
    for fill in fills {
        let letters = fill
            .text
            .chars()
            .filter(|character| !character.is_whitespace())
            .count();
        let lit = fill.sung * letters as f64;
        let mut letter = 0.0;
        let mut tone = rest;
        for character in fill.text.chars() {
            // Spaces take the tone of the letter before them.
            if !character.is_whitespace() {
                tone = rest.towards(Tone::SUNG, lit - letter);
                letter += 1.0;
            }
            characters.push((character, tone));
        }
    }
    let start = characters
        .iter()
        .position(|(character, _)| !character.is_whitespace())
        .unwrap_or(characters.len());
    let end = characters
        .iter()
        .rposition(|(character, _)| !character.is_whitespace())
        .map_or(start, |last| last + 1);
    let mut characters = characters[start..end].to_vec();
    if characters.len() > room {
        characters.truncate(room.saturating_sub(1));
        while characters
            .last()
            .is_some_and(|(character, _)| character.is_whitespace())
        {
            characters.pop();
        }
        characters.push(('…', rest));
    }
    let mut pieces: Vec<Piece> = Vec::new();
    for (character, tone) in characters {
        match pieces.last_mut() {
            Some(last) if last.tone == tone => last.text.push(character),
            _ => pieces.push(piece(character.to_string(), tone)),
        }
    }
    pieces
}

/// An instrumental break being played: its dots light up one after another
/// as the break goes by.
fn break_dots(line: &LyricLine, next: Option<f64>, time: f64, rest: Tone) -> Vec<Piece> {
    let played = share(time - line.time, end_of(line, next) - line.time) * 3.0;
    let mut pieces = Vec::with_capacity(5);
    for dot in 0..3 {
        if dot > 0 {
            pieces.push(piece("  ", rest));
        }
        pieces.push(piece(
            "·",
            rest.towards(Tone::SUNG, played - f64::from(dot)),
        ));
    }
    pieces
}

/// The tone of a line `distance` lines after the line being sung, negative
/// before it: the further away, the dimmer. The line being sung itself has the
/// tone of what is to come.
fn line_tone(distance: isize) -> Tone {
    match distance.unsigned_abs() {
        0 => Tone::COMING,
        1 => Tone::NEAR,
        2 => Tone(33),
        3 => Tone(28),
        _ => Tone::FAR,
    }
}

/// [`line_tone`] while the line being sung comes into view, `arrived` going
/// from 0 to 1: every line comes from the tone it had a line earlier, the line
/// just sung from full brightness.
fn faded(distance: isize, arrived: f64) -> Tone {
    let before = if distance == -1 {
        Tone::SUNG
    } else {
        line_tone(distance + 1)
    };
    before.towards(line_tone(distance), arrived)
}

/// "♪ Lady of Namek – Tory Lanez", the artist shortened first when the
/// terminal is narrow.
fn header(title: &str, artist: &str, width: usize) -> Vec<Piece> {
    let room = width.saturating_sub(4);
    let title_length = title.chars().count();
    let mut pieces = vec![piece("♪ ", Tone::HINT)];
    // The artist needs room for the dash and a few letters.
    if artist.is_empty() || title_length + 7 > room {
        pieces.push(piece(shorten(title, room), Tone::TITLE));
    } else {
        pieces.push(piece(title, Tone::TITLE));
        pieces.push(piece(" – ", Tone::HINT));
        pieces.push(piece(shorten(artist, room - title_length - 3), Tone::NEAR));
    }
    pieces
}

/// "0:35  ━━━━━━────────────  −2:36", as wide as the terminal allows, with ‖
/// in front while the song is paused.
fn time_bar(position: f64, duration: f64, playing: bool, width: usize) -> Vec<Piece> {
    let elapsed = if playing {
        clock(position)
    } else {
        format!("‖ {}", clock(position))
    };
    let remaining = format!("−{}", clock(duration - position));
    let bar = width
        .saturating_sub(elapsed.chars().count() + remaining.chars().count() + 6)
        .min(48);
    let done = ((position / duration).clamp(0.0, 1.0) * bar as f64).round() as usize;
    vec![
        piece(format!("{elapsed}  "), Tone::HINT),
        piece("━".repeat(done), Tone::NEAR),
        piece("─".repeat(bar - done), Tone::FAR),
        piece(format!("  {remaining}"), Tone::HINT),
    ]
}

/// The screen for `now` at `at_ms` in a terminal of `width` × `height`.
pub fn frame(now: Option<&Now>, at_ms: u64, width: u16, height: u16) -> Frame {
    let (width, height) = (usize::from(width), usize::from(height));
    let mut rows: Frame = vec![Vec::new(); height];
    if height == 0 || width < 8 {
        return rows;
    }
    let message = |rows: &mut Frame, text: &str| {
        rows[height / 2] = centred(vec![piece(shorten(text, width - 2), Tone::HINT)], width);
    };
    let Some(now) = now.filter(|now| now::is_fresh(now, at_ms)) else {
        message(&mut rows, "Singline is not running.");
        return rows;
    };
    let Some(title) = now.title.as_deref() else {
        message(&mut rows, "Play a song, and its lyrics show here.");
        return rows;
    };
    rows[0] = centred(
        header(title, now.artist.as_deref().unwrap_or(""), width),
        width,
    );
    let position = now::position_at(now, at_ms);
    if height >= 6 {
        if let (Some(position), Some(duration)) = (position, now.duration.filter(|d| *d > 0.0)) {
            rows[height - 1] = centred(time_bar(position, duration, now.playing, width), width);
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
        rows[middle] = centred(vec![piece("Instrumental", Tone::HINT)], width);
        return rows;
    }
    let (Some(position), true) = (position, lyrics.sync != SyncLevel::Unsynced) else {
        rows[middle] = centred(
            vec![piece("These lyrics have no timing.", Tone::HINT)],
            width,
        );
        return rows;
    };
    let time = position + now.offset;
    let lines = &lyrics.lines;
    let current = lyrics_core::lrc::current_index(lines, time);
    // Before the first line, the lines to come wait below the middle.
    let anchor = current.map_or(-1, |index| index as isize);
    // How far the line being sung has come into view, from 0 to 1.
    let arrived = current.map_or(1.0, |index| {
        ((time - lines[index].time) / FADE).clamp(0.0, 1.0)
    });
    let room = width - 4;
    for row in area {
        let index = anchor + row as isize - middle as isize;
        let Some(line) = usize::try_from(index)
            .ok()
            .and_then(|index| lines.get(index))
        else {
            continue;
        };
        let distance = index - anchor;
        let tone = if current.is_some() {
            faded(distance, arrived)
        } else {
            line_tone(distance)
        };
        let next = lines.get(index as usize + 1).map(|line| line.time);
        let blank = line.text.trim().is_empty();
        let pieces = match (distance, blank) {
            (0, true) => break_dots(line, next, time, tone),
            (0, false) => sung_line(&fills(line, next, time), tone, room),
            (_, true) => vec![piece(DOTS, tone)],
            (_, false) => vec![piece(shorten(line.text.trim(), room), tone)],
        };
        rows[row] = centred(pieces, width);
    }
    rows
}

/// How the view puts colour on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Depth {
    /// Any colour, as most terminals today show.
    True,
    /// The 256 colours of xterm.
    Ansi256,
    /// No colour, as `NO_COLOR` asks: bold and dim text instead.
    Plain,
}

/// The terminal's text and background colours, which every tone is blended
/// from, and how many colours it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Palette {
    text: (u8, u8, u8),
    background: (u8, u8, u8),
    depth: Depth,
}

impl Palette {
    /// Asks the terminal for its colours, assuming white on black when it does
    /// not say. Call it before the view takes the terminal over, since the
    /// answer arrives as input.
    fn detect() -> Self {
        let depth = if std::env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty()) {
            Depth::Plain
        } else if matches!(
            std::env::var("COLORTERM").as_deref(),
            Ok("truecolor" | "24bit")
        ) {
            Depth::True
        } else {
            Depth::Ansi256
        };
        let mut palette = Self {
            text: (255, 255, 255),
            background: (0, 0, 0),
            depth,
        };
        if depth != Depth::Plain {
            let mut options = terminal_colorsaurus::QueryOptions::default();
            options.timeout = COLOUR_QUERY;
            if let Ok(colours) = terminal_colorsaurus::color_palette(options) {
                palette.text = colours.foreground.scale_to_8bit();
                palette.background = colours.background.scale_to_8bit();
            }
        }
        palette
    }

    /// The colour `tone` of the way from the background to the text colour.
    fn blend(&self, tone: Tone) -> (u8, u8, u8) {
        let amount = f64::from(tone.0.min(100)) / 100.0;
        let mix = |from: u8, to: u8| {
            (f64::from(from) + (f64::from(to) - f64::from(from)) * amount).round() as u8
        };
        (
            mix(self.background.0, self.text.0),
            mix(self.background.1, self.text.1),
            mix(self.background.2, self.text.2),
        )
    }

    /// How `tone` shows: as a colour, or in a plain terminal as bold or dim text.
    fn style(&self, tone: Tone) -> (Option<Color>, Option<Attribute>) {
        match self.depth {
            Depth::Plain if tone.0 >= 80 => (None, Some(Attribute::Bold)),
            Depth::Plain if tone.0 < 40 => (None, Some(Attribute::Dim)),
            Depth::Plain => (None, None),
            Depth::True => {
                let (r, g, b) = self.blend(tone);
                (Some(Color::Rgb { r, g, b }), None)
            }
            Depth::Ansi256 => (Some(Color::AnsiValue(nearest_256(self.blend(tone)))), None),
        }
    }
}

/// The colour of the xterm palette nearest to `colour`: from its 6 × 6 × 6
/// cube or from its 24 greys.
fn nearest_256(colour: (u8, u8, u8)) -> u8 {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let step = |value: u8| {
        (0..STEPS.len())
            .min_by_key(|index| u8::abs_diff(STEPS[*index], value))
            .unwrap_or(0)
    };
    let distance = |(r, g, b): (u8, u8, u8)| {
        let square = |a: u8, b: u8| u32::from(a.abs_diff(b)).pow(2);
        square(r, colour.0) + square(g, colour.1) + square(b, colour.2)
    };
    let (r, g, b) = (step(colour.0), step(colour.1), step(colour.2));
    let average = (u16::from(colour.0) + u16::from(colour.1) + u16::from(colour.2)) / 3;
    let grey = ((average.saturating_sub(8) + 5) / 10).min(23) as u8;
    let grey_value = 8 + 10 * grey;
    if distance((grey_value, grey_value, grey_value)) < distance((STEPS[r], STEPS[g], STEPS[b])) {
        232 + grey
    } else {
        (16 + 36 * r + 6 * g + b) as u8
    }
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

/// Draws the rows of `frame` that differ from `shown`, all of them without it.
fn draw(
    out: &mut impl Write,
    frame: &Frame,
    shown: Option<&Frame>,
    palette: &Palette,
) -> io::Result<()> {
    queue!(out, BeginSynchronizedUpdate)?;
    for (row, pieces) in frame.iter().enumerate() {
        if shown.and_then(|shown| shown.get(row)) == Some(pieces) {
            continue;
        }
        let row = u16::try_from(row).unwrap_or(u16::MAX);
        queue!(
            out,
            cursor::MoveTo(0, row),
            terminal::Clear(terminal::ClearType::CurrentLine)
        )?;
        for piece in pieces {
            let (colour, attribute) = palette.style(piece.tone);
            if let Some(colour) = colour {
                queue!(out, SetForegroundColor(colour))?;
            }
            if let Some(attribute) = attribute {
                queue!(out, SetAttribute(attribute))?;
            }
            queue!(out, Print(&piece.text), SetAttribute(Attribute::Reset))?;
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
    let palette = Palette::detect();
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
            draw(&mut out, &frame, shown.as_ref(), &palette)?;
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

    /// The pieces of a row after the spaces that centre it.
    fn shown(row: &[Piece]) -> Vec<(&str, u8)> {
        row.iter()
            .filter(|piece| !piece.text.trim().is_empty())
            .map(|piece| (piece.text.as_str(), piece.tone.0))
            .collect()
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
            playing: true,
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
    fn the_line_being_sung_sits_in_the_middle_and_lights_up_as_it_is_sung() {
        let rows = frame(Some(&song(10.7)), 10_000, 60, 12);
        assert_eq!(text(&rows[0]), "♪ Lady of Namek – Tory Lanez");
        // Rows 2 to 9 hold the lyrics; the middle one is row 5. "can" has
        // just begun: its first letter is half lit.
        assert_eq!(
            shown(&rows[5]),
            [("So, ", 100), ("c", 78), ("an we?", Tone::COMING.0)]
        );
        assert_eq!(
            shown(&rows[6]),
            [("Play with the stars tonight?", Tone::NEAR.0)]
        );
        // Further away, the lines grow dimmer.
        assert_eq!(shown(&rows[7]), [(DOTS, 33)]);
        assert_eq!(shown(&rows[8]), [("Can we make them all align?", 28)]);
        assert!(text(&rows[11]).starts_with("0:10"));
        assert!(text(&rows[11]).ends_with("−3:03"));
    }

    #[test]
    fn a_new_line_fades_in_while_the_line_before_fades_out() {
        // A tenth of a second into "Play with…", a third of the fade.
        let rows = frame(Some(&song(12.1)), 10_000, 60, 12);
        assert_eq!(shown(&rows[4]), [("So, can we?", 81)]);
        assert_eq!(shown(&rows[5]).last().map(|piece| piece.1), Some(47));
        assert_eq!(shown(&rows[6]), [(DOTS, 36)]);
        // Once the fade is over, every line has its own tone.
        let settled = frame(Some(&song(12.5)), 10_000, 60, 12);
        assert_eq!(shown(&settled[4]), [("So, can we?", Tone::NEAR.0)]);
        assert_eq!(
            shown(&settled[5]).last().map(|piece| piece.1),
            Some(Tone::COMING.0)
        );
        assert_eq!(shown(&settled[6]), [(DOTS, Tone::NEAR.0)]);
    }

    #[test]
    fn the_dots_of_a_break_light_up_as_it_goes_by() {
        // Halfway through the break from 15 s to 18 s.
        let rows = frame(Some(&song(16.5)), 10_000, 60, 12);
        assert_eq!(
            shown(&rows[5]),
            [("·", 100), ("·", 78), ("·", Tone::COMING.0)]
        );
        assert_eq!(text(&rows[5]), DOTS);
    }

    #[test]
    fn before_the_first_line_the_lyrics_wait_below_the_middle() {
        let rows = frame(Some(&song(2.0)), 10_000, 60, 12);
        assert_eq!(text(&rows[5]), "");
        assert_eq!(shown(&rows[6]), [("So, can we?", Tone::NEAR.0)]);
    }

    #[test]
    fn a_paused_song_says_so_at_the_time() {
        let mut paused = song(10.7);
        paused.playing = false;
        let rows = frame(Some(&paused), 10_000, 60, 12);
        assert!(text(&rows[11]).starts_with("‖ 0:10"));
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
        for position in [10.7, 12.5, 13.4, 16.0] {
            let rows = frame(Some(&song(position)), 10_000, 20, 12);
            assert!(rows.iter().all(|row| text(row).chars().count() <= 20));
        }
        let rows = frame(Some(&song(12.5)), 10_000, 20, 12);
        assert!(text(&rows[5]).ends_with('…'));
        assert_eq!(text(&rows[0]), "♪ Lady of Namek");
    }

    #[test]
    fn estimates_the_words_of_lines_without_word_timing_like_the_overlay() {
        let line = LyricLine::new(12.0, "Play with the stars tonight?");
        let sung = |time: f64| -> Vec<f64> {
            fills(&line, Some(15.0), time)
                .iter()
                .map(|fill| (fill.sung * 100.0).round() / 100.0)
                .collect()
        };
        let joined: String = fills(&line, Some(15.0), 12.0)
            .into_iter()
            .map(|fill| fill.text)
            .collect();
        assert_eq!(joined, "Play with the stars tonight?");
        assert_eq!(sung(12.0), [0.0; 5]);
        // 0.07 s per letter and word: "Play " and "with " take 0.35 s each,
        // "the " 0.28 s, so "stars " has just begun at 13 s.
        assert_eq!(sung(13.0), [1.0, 1.0, 1.0, 0.05, 0.0]);
        assert_eq!(sung(14.1), [1.0; 5]);
    }

    #[test]
    fn timed_words_fill_up_until_the_next_word() {
        let Some(lyrics) = song(0.0).lyrics else {
            panic!("the song has lyrics");
        };
        let sung: Vec<f64> = fills(&lyrics.lines[0], Some(12.0), 10.9)
            .iter()
            .map(|fill| fill.sung)
            .collect();
        assert_eq!(sung[0], 1.0);
        assert!((sung[1] - 0.5).abs() < 1e-9);
        assert_eq!(sung[2], 0.0);
    }

    #[test]
    fn splits_text_into_words_with_their_spaces() {
        assert_eq!(words("  Play  with me "), ["  Play  ", "with ", "me "]);
        assert!(words("   ").is_empty());
    }

    #[test]
    fn blends_tones_from_the_terminal_colours() {
        let dark = Palette {
            text: (230, 230, 230),
            background: (20, 20, 20),
            depth: Depth::True,
        };
        assert_eq!(dark.blend(Tone::SUNG), (230, 230, 230));
        assert_eq!(dark.blend(Tone(0)), (20, 20, 20));
        let light = Palette {
            text: (0, 0, 0),
            background: (255, 255, 255),
            depth: Depth::True,
        };
        // On a light theme the lines further away fade towards white.
        assert_eq!(light.blend(Tone::FAR), (194, 194, 194));
        assert_eq!(
            light.style(Tone::SUNG),
            (Some(Color::Rgb { r: 0, g: 0, b: 0 }), None)
        );
        let plain = Palette {
            depth: Depth::Plain,
            ..dark
        };
        assert_eq!(plain.style(Tone::SUNG), (None, Some(Attribute::Bold)));
        assert_eq!(plain.style(Tone::FAR), (None, Some(Attribute::Dim)));
        assert_eq!(plain.style(Tone::COMING), (None, None));
    }

    #[test]
    fn finds_the_nearest_of_the_256_colours() {
        assert_eq!(nearest_256((0, 0, 0)), 16);
        assert_eq!(nearest_256((255, 255, 255)), 231);
        assert_eq!(nearest_256((128, 128, 128)), 244);
        assert_eq!(nearest_256((255, 0, 0)), 196);
    }

    #[test]
    fn only_changed_rows_are_drawn_again() {
        let palette = Palette {
            text: (255, 255, 255),
            background: (0, 0, 0),
            depth: Depth::Plain,
        };
        let before = frame(Some(&song(12.5)), 10_000, 60, 12);
        let after = frame(Some(&song(12.6)), 10_000, 60, 12);
        let mut out = Vec::new();
        draw(&mut out, &after, Some(&before), &palette).unwrap_or_else(|error| panic!("{error}"));
        let written = String::from_utf8_lossy(&out);
        // The line being sung changed; the song and the other lines did not.
        assert!(written.contains("Play"));
        assert!(!written.contains("Lady of Namek"));
        assert!(!written.contains("Can we make"));
    }

    #[test]
    fn tiny_terminals_get_an_empty_screen() {
        assert!(frame(Some(&song(10.0)), 10_000, 4, 3)
            .iter()
            .all(Vec::is_empty));
    }
}
