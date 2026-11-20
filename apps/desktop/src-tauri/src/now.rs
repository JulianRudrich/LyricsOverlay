//! What plays, for the terminal. Singline keeps `now.json` up to date in its
//! cache folder; `singline now`, the Pi extension and other status lines read
//! it. The file holds the position at one moment, so readers move it on by
//! themselves. It is rewritten when anything else changes, after a seek, and
//! every few seconds, so readers can tell when Singline has quit.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lyrics_core::Lyrics;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::playback::{LyricsView, PlaybackView};
use crate::AppState;

/// The format of `now.json`; readers ignore files of a version they do not know.
pub const VERSION: u32 = 1;
/// Readers take a file this old for a Singline that has quit.
pub const STALE_AFTER: Duration = Duration::from_secs(20);
/// While a song is loaded, the file is rewritten at least this often.
const HEARTBEAT: Duration = Duration::from_secs(5);
/// A position this far from where the last one would be by now means a seek.
const SEEK: f64 = 0.4;
const IDENTIFIER: &str = "io.github.julianrudrich.singline";

/// The contents of `now.json`. Times are seconds into the song, moments are
/// milliseconds since 1970.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Now {
    pub version: u32,
    pub updated_at: u64,
    pub player: Option<String>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub playing: bool,
    /// Seconds into the song at `at`.
    pub position: Option<f64>,
    pub at: u64,
    pub rate: f64,
    /// Seconds the lyrics show earlier: the global and the song's correction.
    pub offset: f64,
    pub lyrics: Option<Lyrics>,
}

/// Where `now.json` lives: in the cache folder, or in `SINGLINE_DATA_DIR` when
/// that is set, as in the tests.
pub fn path() -> Option<PathBuf> {
    if let Some(directory) = std::env::var_os("SINGLINE_DATA_DIR") {
        return Some(PathBuf::from(directory).join("now.json"));
    }
    cache_dir().map(|directory| directory.join(IDENTIFIER).join("now.json"))
}

#[cfg(target_os = "macos")]
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Caches"))
}

#[cfg(windows)]
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|directory| directory.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
}

fn epoch_ms(moment: SystemTime) -> u64 {
    moment
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Everything but the position: when this changes, the file is rewritten.
#[derive(Debug, Clone, PartialEq)]
struct Signature {
    player: Option<String>,
    track: Option<String>,
    playing: bool,
    rate: f64,
    offset: f64,
    lyrics: Option<(String, &'static str, usize)>,
}

/// What was written last.
#[derive(Debug, Clone)]
struct Written {
    signature: Signature,
    position: Option<f64>,
    at: Instant,
}

#[derive(Default)]
pub struct Publisher {
    last: Mutex<Option<Written>>,
}

/// Whether the file needs writing: something changed, the position jumped, or
/// the heartbeat is due while a song is loaded.
fn due(last: Option<&Written>, signature: &Signature, position: Option<f64>, now: Instant) -> bool {
    let Some(last) = last else {
        return true;
    };
    if last.signature != *signature {
        return true;
    }
    let elapsed = now.saturating_duration_since(last.at);
    if signature.track.is_some() && elapsed >= HEARTBEAT {
        return true;
    }
    match (last.position, position) {
        (Some(before), Some(after)) => {
            let moved = if signature.playing {
                elapsed.as_secs_f64() * signature.rate
            } else {
                0.0
            };
            (before + moved - after).abs() > SEEK
        }
        (before, after) => before.is_some() != after.is_some(),
    }
}

fn signature(playback: &PlaybackView, lyrics: &LyricsView, offset: f64) -> Signature {
    let track = playback.track.as_ref().map(|track| track.key.clone());
    Signature {
        player: playback.player.as_ref().map(|player| player.name.clone()),
        playing: playback.playing,
        rate: playback.rate,
        offset,
        lyrics: lyrics
            .track_key
            .clone()
            .filter(|key| Some(key) == track.as_ref())
            .map(|key| {
                let lines = lyrics
                    .lyrics
                    .as_ref()
                    .map_or(0, |lyrics| lyrics.lines.len());
                (key, lyrics.status, lines)
            }),
        track,
    }
}

fn snapshot(playback: &PlaybackView, lyrics: &LyricsView, offset: f64, now: Instant) -> Now {
    let wall = SystemTime::now();
    let track = playback.track.as_ref();
    let own = lyrics.track_key.is_some() && lyrics.track_key.as_ref() == track.map(|t| &t.key);
    Now {
        version: VERSION,
        updated_at: epoch_ms(wall),
        player: playback.player.as_ref().map(|player| player.name.clone()),
        title: track.map(|track| track.title.clone()),
        artist: track.map(|track| track.artist.clone()),
        album: track.and_then(|track| track.album.clone()),
        duration: track.and_then(|track| track.duration),
        playing: playback.playing,
        position: playback.position_at(now),
        at: epoch_ms(wall),
        rate: playback.rate,
        offset,
        lyrics: if own { lyrics.lyrics.clone() } else { None },
    }
}

/// Writes the file when the song, the lyrics or the position changed in a way
/// readers cannot work out themselves. Cheap otherwise: the player loop calls
/// it on every reading.
pub fn update(app: &AppHandle) {
    let (Some(publisher), Some(state)) =
        (app.try_state::<Publisher>(), app.try_state::<AppState>())
    else {
        return;
    };
    if state.demo {
        return;
    }
    let offset = state.settings().global_offset;
    let playback = state
        .playback
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let lyrics = state.lyrics_view.lock().unwrap_or_else(|p| p.into_inner());
    let offset = offset + lyrics.song_offset;
    let now = Instant::now();
    let signature = signature(&playback, &lyrics, offset);
    let position = playback.position_at(now);
    let mut last = publisher.last.lock().unwrap_or_else(|p| p.into_inner());
    if !due(last.as_ref(), &signature, position, now) {
        return;
    }
    let contents = snapshot(&playback, &lyrics, offset, now);
    drop(lyrics);
    let Some(path) = path() else {
        return;
    };
    match write(&path, &contents) {
        Ok(()) => {
            *last = Some(Written {
                signature,
                position,
                at: now,
            })
        }
        Err(error) => eprintln!("could not write {}: {error}", path.display()),
    }
}

/// Leaves a file without a song, so status lines go quiet when Singline quits.
pub fn clear(app: &AppHandle) {
    let demo = app.try_state::<AppState>().is_none_or(|state| state.demo);
    if demo {
        return;
    }
    let Some(path) = path() else {
        return;
    };
    let wall = epoch_ms(SystemTime::now());
    let empty = Now {
        version: VERSION,
        updated_at: wall,
        player: None,
        title: None,
        artist: None,
        album: None,
        duration: None,
        playing: false,
        position: None,
        at: wall,
        rate: 1.0,
        offset: 0.0,
        lyrics: None,
    };
    let _ = write(&path, &empty);
}

/// Replaces the file in one step, so a reader never sees half of it.
fn write(path: &Path, now: &Now) -> std::io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let text = serde_json::to_string(now).map_err(std::io::Error::other)?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

const HELP: &str = "Usage: singline now [--format TEXT]

Prints the lyric line being sung right now, for tmux, your prompt or another
status line, and nothing while no song plays. Singline must be running.

  --format TEXT  What to print, with {line}, {next}, {title}, {artist},
                 {album} and {player} filled in.
                 Default: \"♪ {line}\", or \"♪ {title} – {artist}\" between lines.";

/// `singline now`: prints the current line and returns the exit code.
pub fn print(arguments: &[String]) -> i32 {
    let mut format = None;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--format" | "-f" => match rest.next() {
                Some(value) => format = Some(value.clone()),
                None => {
                    eprintln!("--format needs a text.\n\n{HELP}");
                    return 2;
                }
            },
            "--help" | "-h" => {
                println!("{HELP}");
                return 0;
            }
            other => {
                eprintln!("Unknown option {other}.\n\n{HELP}");
                return 2;
            }
        }
    }
    let now = path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Now>(&text).ok());
    if let Some(line) =
        now.and_then(|now| render(&now, epoch_ms(SystemTime::now()), format.as_deref()))
    {
        println!("{line}");
    }
    0
}

/// The text to print at `at_ms`, or nothing for an old file or without a song.
fn render(now: &Now, at_ms: u64, format: Option<&str>) -> Option<String> {
    if now.version != VERSION
        || at_ms.saturating_sub(now.updated_at) > STALE_AFTER.as_millis() as u64
    {
        return None;
    }
    let title = now.title.as_deref()?;
    let artist = now.artist.as_deref().unwrap_or("");
    let (line, next) = lines_at(now, at_ms);
    let format = format.unwrap_or(if line.is_empty() {
        "♪ {title} – {artist}"
    } else {
        "♪ {line}"
    });
    let text = format
        .replace("{line}", line)
        .replace("{next}", next)
        .replace("{title}", title)
        .replace("{artist}", artist)
        .replace("{album}", now.album.as_deref().unwrap_or(""))
        .replace("{player}", now.player.as_deref().unwrap_or(""));
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The position at `at_ms`: moved on while the song plays, never past its end.
pub fn position_at(now: &Now, at_ms: u64) -> Option<f64> {
    let position = now.position?;
    let elapsed = if now.playing {
        at_ms.saturating_sub(now.at) as f64 / 1000.0 * now.rate
    } else {
        0.0
    };
    let moved = (position + elapsed).max(0.0);
    Some(match now.duration {
        Some(duration) if duration > 0.0 => moved.min(duration),
        _ => moved,
    })
}

/// The line being sung at `at_ms` and the next one, empty where there is none:
/// before the first line, in a break, or for lyrics without timing.
fn lines_at(now: &Now, at_ms: u64) -> (&str, &str) {
    let Some(lyrics) = now.lyrics.as_ref() else {
        return ("", "");
    };
    let Some(position) = position_at(now, at_ms) else {
        return ("", "");
    };
    if lyrics.sync == lyrics_core::SyncLevel::Unsynced {
        return ("", "");
    }
    let lines = &lyrics.lines;
    let index = lyrics_core::lrc::current_index(lines, position + now.offset);
    let line = index.map_or("", |index| lines[index].text.trim());
    let start = index.map_or(0, |index| index + 1);
    let next = lines[start..]
        .iter()
        .map(|line| line.text.trim())
        .find(|text| !text.is_empty())
        .unwrap_or("");
    (line, next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyrics_core::{LyricLine, SyncLevel};

    fn now_playing(position: f64, playing: bool) -> Now {
        Now {
            version: VERSION,
            updated_at: 10_000,
            player: Some("Spotify".into()),
            title: Some("Lady of Namek".into()),
            artist: Some("Tory Lanez".into()),
            album: None,
            duration: Some(194.0),
            playing,
            position: Some(position),
            at: 10_000,
            rate: 1.0,
            offset: 0.0,
            lyrics: Some(Lyrics {
                lines: vec![
                    LyricLine::new(10.0, "So, can we?"),
                    LyricLine::new(12.0, "Play with the stars tonight?"),
                    LyricLine::new(15.0, ""),
                    LyricLine::new(18.0, "Can we make them all align?"),
                ],
                sync: SyncLevel::Line,
                instrumental: false,
            }),
        }
    }

    #[test]
    fn prints_the_line_being_sung_and_moves_on_while_playing() {
        let now = now_playing(11.0, true);
        assert_eq!(render(&now, 10_000, None).as_deref(), Some("♪ So, can we?"));
        assert_eq!(
            render(&now, 11_500, None).as_deref(),
            Some("♪ Play with the stars tonight?")
        );
    }

    #[test]
    fn a_paused_song_stays_on_its_line() {
        let now = now_playing(11.0, false);
        assert_eq!(render(&now, 19_000, None).as_deref(), Some("♪ So, can we?"));
    }

    #[test]
    fn shows_the_song_before_the_first_line_and_in_breaks() {
        let now = now_playing(5.0, false);
        assert_eq!(
            render(&now, 10_000, None).as_deref(),
            Some("♪ Lady of Namek – Tory Lanez")
        );
        let now = now_playing(16.0, false);
        assert_eq!(
            render(&now, 10_000, None).as_deref(),
            Some("♪ Lady of Namek – Tory Lanez")
        );
    }

    #[test]
    fn the_correction_shows_lines_earlier() {
        let mut now = now_playing(11.7, false);
        now.offset = 0.5;
        assert_eq!(
            render(&now, 10_000, None).as_deref(),
            Some("♪ Play with the stars tonight?")
        );
    }

    #[test]
    fn fills_in_a_format_with_the_next_line() {
        let now = now_playing(12.5, false);
        assert_eq!(
            render(&now, 10_000, Some("{artist}: {line} / {next} ({player})")).as_deref(),
            Some(
                "Tory Lanez: Play with the stars tonight? / Can we make them all align? (Spotify)"
            )
        );
    }

    #[test]
    fn says_nothing_for_an_old_file_or_without_a_song() {
        let now = now_playing(11.0, true);
        assert_eq!(render(&now, 10_000 + 21_000, None), None);
        let mut quiet = now_playing(11.0, false);
        quiet.title = None;
        assert_eq!(render(&quiet, 10_000, None), None);
    }

    #[test]
    fn lyrics_without_timing_show_the_song() {
        let mut now = now_playing(11.0, false);
        if let Some(lyrics) = now.lyrics.as_mut() {
            lyrics.sync = SyncLevel::Unsynced;
        }
        assert_eq!(
            render(&now, 10_000, None).as_deref(),
            Some("♪ Lady of Namek – Tory Lanez")
        );
    }

    #[test]
    fn the_position_stops_at_the_end_of_the_song() {
        let now = now_playing(190.0, true);
        assert_eq!(position_at(&now, 20_000), Some(194.0));
    }

    fn signature(playing: bool) -> Signature {
        Signature {
            player: Some("Spotify".into()),
            track: Some("lady".into()),
            playing,
            rate: 1.0,
            offset: 0.0,
            lyrics: None,
        }
    }

    #[test]
    fn writes_on_changes_seeks_and_the_heartbeat_only() {
        let start = Instant::now();
        let last = Written {
            signature: signature(true),
            position: Some(10.0),
            at: start,
        };
        let later = start + Duration::from_secs(2);
        // Playing on as expected: nothing to write.
        assert!(!due(Some(&last), &signature(true), Some(12.0), later));
        // A seek.
        assert!(due(Some(&last), &signature(true), Some(40.0), later));
        // Paused.
        assert!(due(Some(&last), &signature(false), Some(12.0), later));
        // The heartbeat.
        assert!(due(
            Some(&last),
            &signature(true),
            Some(16.0),
            start + Duration::from_secs(6)
        ));
        // The first time.
        assert!(due(None, &signature(true), Some(10.0), start));
    }

    #[test]
    fn the_file_round_trips() {
        let now = now_playing(11.0, true);
        let text = serde_json::to_string(&now).expect("serialises");
        assert!(text.contains("\"updatedAt\":10000"));
        assert_eq!(serde_json::from_str::<Now>(&text).expect("parses"), now);
    }
}
