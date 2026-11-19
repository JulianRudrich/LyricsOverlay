//! Reading Spotify and Apple's Music app on macOS through AppleScript.
//!
//! One `osascript` run reads every running player. Its output uses the ASCII
//! record and unit separators, so titles with commas or quotes survive, and
//! times are sent as whole milliseconds, so the region's decimal separator
//! cannot garble them. A player that is not running is never addressed, so it
//! is never launched.
//!
//! Building the script and parsing its output are plain functions, tested on
//! every system. Only running it is macOS-specific.

use std::time::Instant;

use crate::model::{
    Capabilities, Command, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Problem, Reading,
    Service, Track,
};
use crate::normalize::track_key;

const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// AppleScript error codes the players answer with.
const NOT_AUTHORIZED: i64 = -1743;
const NO_SUCH_OBJECT: i64 = -1728;
const APP_NOT_RUNNING: i64 = -600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacPlayer {
    Spotify,
    Music,
}

impl MacPlayer {
    pub const ALL: [MacPlayer; 2] = [MacPlayer::Spotify, MacPlayer::Music];

    pub fn bundle_id(self) -> &'static str {
        match self {
            MacPlayer::Spotify => "com.spotify.client",
            MacPlayer::Music => "com.apple.Music",
        }
    }

    pub fn id(self) -> PlayerId {
        PlayerId::new(format!("applescript:{}", self.bundle_id()))
    }

    pub fn from_id(id: &PlayerId) -> Option<Self> {
        Self::ALL.into_iter().find(|player| player.id() == *id)
    }

    pub fn info(self) -> PlayerInfo {
        let (name, service) = match self {
            MacPlayer::Spotify => ("Spotify", Service::Spotify),
            MacPlayer::Music => ("Apple Music", Service::AppleMusic),
        };
        PlayerInfo {
            id: self.id(),
            name: name.into(),
            service,
            web: false,
        }
    }

    /// The executable a running instance shows in `ps`.
    fn executable(self) -> &'static str {
        match self {
            MacPlayer::Spotify => "Spotify.app/Contents/MacOS/Spotify",
            MacPlayer::Music => "Music.app/Contents/MacOS/Music",
        }
    }

    fn handler(self) -> &'static str {
        match self {
            MacPlayer::Spotify => SPOTIFY_HANDLER,
            MacPlayer::Music => MUSIC_HANDLER,
        }
    }

    fn handler_call(self) -> &'static str {
        match self {
            MacPlayer::Spotify => "readSpotify(fs)",
            MacPlayer::Music => "readMusic(fs)",
        }
    }
}

const SPOTIFY_HANDLER: &str = r#"
on readSpotify(fs)
	if application id "com.spotify.client" is not running then return "closed"
	try
		with timeout of 2 seconds
			tell application id "com.spotify.client"
				set stateText to (player state as text)
				if stateText is "stopped" then return "stopped"
				set t to current track
				set songTitle to name of t
				set songArtist to artist of t
				set songDuration to duration of t
				set songPosition to player position
				if songTitle is missing value or songArtist is missing value or songDuration is missing value or songPosition is missing value then return "nodata" & fs & stateText
				set trackID to ""
				try
					set trackID to (id of t) as text
				end try
				set albumName to ""
				try
					set albumValue to album of t
					if albumValue is not missing value then set albumName to albumValue as text
				end try
				set artURL to ""
				try
					set artValue to artwork url of t
					if artValue is not missing value then set artURL to artValue as text
				end try
				return "ok" & fs & stateText & fs & trackID & fs & (songTitle as text) & fs & (songArtist as text) & fs & albumName & fs & (songDuration div 1) & fs & ((songPosition * 1000) div 1) & fs & artURL
			end tell
		end timeout
	on error errorMessage number errorNumber
		return "error" & fs & errorNumber
	end try
end readSpotify
"#;

const MUSIC_HANDLER: &str = r#"
on readMusic(fs)
	if application id "com.apple.Music" is not running then return "closed"
	try
		with timeout of 2 seconds
			tell application id "com.apple.Music"
				set stateText to (player state as text)
				if stateText is "stopped" then return "stopped"
				set t to current track
				set songTitle to name of t
				set songArtist to artist of t
				set songDuration to duration of t
				set songPosition to player position
				if songTitle is missing value or songArtist is missing value or songDuration is missing value or songPosition is missing value then return "nodata" & fs & stateText
				set trackID to ""
				try
					set trackID to (persistent ID of t) as text
				end try
				set albumName to ""
				try
					set albumValue to album of t
					if albumValue is not missing value then set albumName to albumValue as text
				end try
				return "ok" & fs & stateText & fs & trackID & fs & (songTitle as text) & fs & (songArtist as text) & fs & albumName & fs & ((songDuration * 1000) div 1) & fs & ((songPosition * 1000) div 1) & fs & ""
			end tell
		end timeout
	on error errorMessage number errorNumber
		return "error" & fs & errorNumber
	end try
end readMusic
"#;

/// The players a `ps -axww -o comm=` listing shows as running.
pub fn running_players(ps_output: &str) -> Vec<MacPlayer> {
    MacPlayer::ALL
        .into_iter()
        .filter(|player| {
            ps_output.lines().map(str::trim).any(|line| {
                line.ends_with(&format!("/{}", player.executable())) || line == player.executable()
            })
        })
        .collect()
}

/// A script that reads `players`, in this order, separated by the record separator.
pub fn read_script(players: &[MacPlayer]) -> String {
    let mut script = String::from(
        "on run\n\tset fs to character id 31\n\tset rs to character id 30\n\tset parts to {}\n",
    );
    for player in players {
        script.push_str(&format!(
            "\tset end of parts to my {}\n",
            player.handler_call()
        ));
    }
    script.push_str("\tset AppleScript's text item delimiters to rs\n\tset joined to parts as text\n\tset AppleScript's text item delimiters to \"\"\n\treturn joined\nend run\n");
    for player in players {
        script.push_str(player.handler());
    }
    script
}

/// A script that sends `command` to `player`, but only while it runs.
pub fn command_script(player: MacPlayer, command: Command) -> String {
    let action = match command {
        Command::PlayPause => "playpause".to_owned(),
        Command::Pause => "pause".to_owned(),
        Command::Next => "next track".to_owned(),
        Command::Previous => "previous track".to_owned(),
        Command::SeekTo { position } => {
            let position = if position.is_finite() {
                position.max(0.0)
            } else {
                0.0
            };
            format!("set player position to {position:.3}")
        }
    };
    let id = player.bundle_id();
    format!("if application id \"{id}\" is running then\n\ttell application id \"{id}\" to {action}\nend if\n")
}

/// Turns the output of [`read_script`] into readings, one field list per player.
pub fn parse_output(output: &str, players: &[MacPlayer], sampled_at: Instant) -> Vec<Reading> {
    let output = output.strip_suffix('\n').unwrap_or(output);
    let mut records = output.split(RECORD);
    players
        .iter()
        .filter_map(|player| {
            let record = records.next().unwrap_or("error");
            parse_record(*player, record, sampled_at)
        })
        .collect()
}

fn parse_record(player: MacPlayer, record: &str, sampled_at: Instant) -> Option<Reading> {
    let fields: Vec<&str> = record.split(FIELD).collect();
    let unavailable = |problem: Problem, playing: Option<bool>| Reading::Unavailable {
        player: player.info(),
        problem,
        playing,
    };
    match fields.first().copied() {
        Some("closed") => None,
        Some("stopped") => Some(Reading::Snapshot(PlayerSnapshot {
            player: player.info(),
            status: PlaybackStatus::Stopped,
            track: None,
            position: None,
            sampled_at,
            rate: 1.0,
            capabilities: capabilities(),
        })),
        Some("nodata") => Some(unavailable(
            Problem::NoTrackData,
            fields
                .get(1)
                .map(|state| status(state) == PlaybackStatus::Playing),
        )),
        Some("error") => match fields
            .get(1)
            .and_then(|code| code.trim().parse::<i64>().ok())
        {
            Some(APP_NOT_RUNNING) => None,
            Some(NOT_AUTHORIZED) => Some(unavailable(Problem::PermissionDenied, None)),
            Some(NO_SUCH_OBJECT) => Some(unavailable(Problem::NoTrackData, None)),
            code => Some(unavailable(
                Problem::ReadFailed {
                    detail: code.map_or_else(
                        || "no answer".to_owned(),
                        |code| format!("AppleScript error {code}"),
                    ),
                },
                None,
            )),
        },
        Some("ok") if fields.len() >= 9 => {
            let [_, state, id, title, artist, album, duration, position, artwork] = fields[..9]
            else {
                return None;
            };
            let title = title.trim();
            let artist = artist.trim();
            let album = Some(album.trim()).filter(|album| !album.is_empty());
            let milliseconds = |value: &str| {
                value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map(|v| v / 1_000.0)
            };
            let source_id = Some(id.trim())
                .filter(|id| !id.is_empty())
                .map(|id| match player {
                    MacPlayer::Spotify => id.to_owned(),
                    MacPlayer::Music => format!("apple-music:{id}"),
                });
            let track = (!title.is_empty()).then(|| Track {
                key: track_key(title, artist, album),
                source_id,
                title: title.to_owned(),
                artist: artist.to_owned(),
                album: album.map(str::to_owned),
                duration: milliseconds(duration).filter(|d| *d > 0.0),
                artwork_url: Some(artwork.trim())
                    .filter(|url| url.starts_with("https://"))
                    .map(str::to_owned),
            });
            Some(Reading::Snapshot(PlayerSnapshot {
                player: player.info(),
                status: status(state),
                position: track.as_ref().and(milliseconds(position)),
                track,
                sampled_at,
                rate: 1.0,
                capabilities: capabilities(),
            }))
        }
        _ => Some(unavailable(
            Problem::ReadFailed {
                detail: "unexpected answer".into(),
            },
            None,
        )),
    }
}

fn status(state: &str) -> PlaybackStatus {
    match state.trim() {
        "playing" | "fast forwarding" | "rewinding" => PlaybackStatus::Playing,
        "paused" => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    }
}

fn capabilities() -> Capabilities {
    Capabilities {
        play_pause: true,
        next: true,
        previous: true,
        seek: true,
    }
}

/// Runs a program and returns its standard output, killing it after `timeout`.
///
/// Both pipes are read on their own threads while the program runs: `ps`
/// lists hundreds of processes on a busy Mac, more than a pipe holds, and a
/// program that cannot write blocks until someone reads.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn run(program: &str, args: &[&str], timeout: std::time::Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command as Process, Stdio};
    use std::thread;

    let mut child = Process::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut text);
            }
            String::from_utf8_lossy(&text).into_owned()
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                // The readers end by themselves once the pipes close; waiting
                // for them could hang on a grandchild that keeps a pipe open.
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => return Err(error.to_string()),
        }
    };
    let output = stdout.join().unwrap_or_default();
    if status.success() {
        return Ok(output);
    }
    let message = stderr.join().unwrap_or_default();
    Err(format!("{status}: {}", message.trim()))
}

#[cfg(target_os = "macos")]
pub use runner::AppleScriptBackend;

#[cfg(target_os = "macos")]
mod runner {
    use std::time::{Duration, Instant};

    use super::{command_script, parse_output, read_script, run, running_players, MacPlayer};
    use crate::backend::{Backend, CommandError};
    use crate::model::{Command, PlayerId, Problem, Reading};

    /// Reads Spotify and Music with `osascript`.
    #[derive(Debug, Default)]
    pub struct AppleScriptBackend {
        last_error: Option<String>,
    }

    impl AppleScriptBackend {
        pub fn new() -> Self {
            Self::default()
        }
    }

    impl Backend for AppleScriptBackend {
        fn name(&self) -> &'static str {
            "applescript"
        }

        fn poll(&mut self) -> Vec<Reading> {
            let listing = match run("/bin/ps", &["-axww", "-o", "comm="], Duration::from_secs(3)) {
                Ok(listing) => listing,
                Err(error) => {
                    self.last_error = Some(format!("ps: {error}"));
                    return Vec::new();
                }
            };
            let players = running_players(&listing);
            if players.is_empty() {
                self.last_error = None;
                return Vec::new();
            }
            match run(
                "/usr/bin/osascript",
                &["-e", &read_script(&players)],
                Duration::from_secs(6),
            ) {
                Ok(output) => {
                    self.last_error = None;
                    parse_output(&output, &players, Instant::now())
                }
                Err(error) => {
                    self.last_error = Some(format!("osascript: {error}"));
                    players
                        .iter()
                        .map(|player| Reading::Unavailable {
                            player: player.info(),
                            problem: Problem::ReadFailed {
                                detail: error.clone(),
                            },
                            playing: None,
                        })
                        .collect()
                }
            }
        }

        fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
            let player = MacPlayer::from_id(player).ok_or(CommandError::PlayerGone)?;
            run(
                "/usr/bin/osascript",
                &["-e", &command_script(player, command)],
                Duration::from_secs(4),
            )
            .map(|_| ())
            .map_err(CommandError::Failed)
        }

        fn poll_interval(&self) -> Duration {
            Duration::from_millis(800)
        }

        fn last_error(&self) -> Option<String> {
            self.last_error.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_reads_output_larger_than_a_pipe() {
        // About 1 MB, far more than a pipe buffer: the old helper hung here.
        let output = run(
            "/bin/sh",
            &[
                "-c",
                "yes /Applications/Spotify.app/Contents/MacOS/Spotify | head -n 20000",
            ],
            std::time::Duration::from_secs(20),
        )
        .expect("the program finishes");
        assert_eq!(output.lines().count(), 20_000);
        assert_eq!(running_players(&output), [MacPlayer::Spotify]);
    }

    #[test]
    fn run_stops_a_program_that_takes_too_long() {
        let started = Instant::now();
        let error = run(
            "/bin/sh",
            &["-c", "sleep 5"],
            std::time::Duration::from_millis(200),
        )
        .unwrap_err();
        assert_eq!(error, "timed out");
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn run_reports_why_a_program_failed() {
        let error = run(
            "/bin/sh",
            &["-c", "echo not allowed >&2; exit 3"],
            std::time::Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(error.ends_with("not allowed"), "{error}");
    }

    fn record(fields: &[&str]) -> String {
        fields.join(&FIELD.to_string())
    }

    #[test]
    fn finds_running_players_in_ps_output() {
        let listing = "/sbin/launchd\n/Applications/Spotify.app/Contents/MacOS/Spotify\n/Applications/Spotify.app/Contents/Frameworks/Spotify Helper.app/Contents/MacOS/Spotify Helper\n";
        assert_eq!(running_players(listing), [MacPlayer::Spotify]);
        let both = "/System/Applications/Music.app/Contents/MacOS/Music\n/Users/me/Applications/Spotify.app/Contents/MacOS/Spotify";
        assert_eq!(
            running_players(both),
            [MacPlayer::Spotify, MacPlayer::Music]
        );
        assert!(running_players("/Applications/MusicBox.app/Contents/MacOS/MusicBox\n").is_empty());
    }

    #[test]
    fn scripts_only_mention_running_players() {
        let script = read_script(&[MacPlayer::Music]);
        assert!(script.contains("readMusic(fs)") && script.contains("on readMusic"));
        assert!(
            !script.contains("com.spotify.client"),
            "Spotify may not be installed"
        );
        let both = read_script(&MacPlayer::ALL);
        assert!(both.find("readSpotify(fs)") < both.find("readMusic(fs)"));
    }

    #[test]
    fn commands_never_launch_a_player() {
        let script = command_script(MacPlayer::Spotify, Command::SeekTo { position: 61.25 });
        assert!(script.starts_with("if application id \"com.spotify.client\" is running then"));
        assert!(script.contains("set player position to 61.250"));
        assert!(
            command_script(MacPlayer::Music, Command::SeekTo { position: f64::NAN })
                .contains("to 0.000")
        );
        assert!(command_script(MacPlayer::Music, Command::Next).contains("to next track"));
        assert_eq!(
            command_script(MacPlayer::Music, Command::Pause),
            "if application id \"com.apple.Music\" is running then\n\ttell application id \"com.apple.Music\" to pause\nend if\n",
            "pause, not playpause, so a paused player stays paused"
        );
    }

    #[test]
    fn parses_a_spotify_reading() {
        let output = record(&[
            "ok",
            "playing",
            "spotify:track:abc",
            "Title, with comma",
            "Artist",
            "Album",
            "215000",
            "12345",
            "https://i.scdn.co/image/x",
        ]) + "\n";
        let now = Instant::now();
        let readings = parse_output(&output, &[MacPlayer::Spotify], now);
        let [Reading::Snapshot(snapshot)] = readings.as_slice() else {
            panic!("expected one snapshot, got {readings:?}");
        };
        assert_eq!(snapshot.status, PlaybackStatus::Playing);
        assert_eq!(snapshot.position, Some(12.345));
        let track = snapshot.track.as_ref().expect("track");
        assert_eq!(
            (
                track.title.as_str(),
                track.artist.as_str(),
                track.album.as_deref()
            ),
            ("Title, with comma", "Artist", Some("Album"))
        );
        assert_eq!(track.duration, Some(215.0));
        assert_eq!(track.source_id.as_deref(), Some("spotify:track:abc"));
        assert_eq!(
            track.artwork_url.as_deref(),
            Some("https://i.scdn.co/image/x")
        );
        assert_eq!(snapshot.player.service, Service::Spotify);
    }

    #[test]
    fn parses_several_players_and_their_problems() {
        let output = [
            record(&["error", "-1743"]),
            record(&[
                "ok", "paused", "ABC123", "Song", "Band", "", "200500", "1000", "",
            ]),
        ]
        .join(&RECORD.to_string());
        let readings = parse_output(&output, &MacPlayer::ALL, Instant::now());
        assert!(matches!(
            &readings[0],
            Reading::Unavailable {
                problem: Problem::PermissionDenied,
                ..
            }
        ));
        let Reading::Snapshot(music) = &readings[1] else {
            panic!("expected a Music snapshot");
        };
        assert_eq!(music.status, PlaybackStatus::Paused);
        let track = music.track.as_ref().expect("track");
        assert_eq!(track.album, None);
        assert_eq!(track.duration, Some(200.5));
        assert_eq!(track.source_id.as_deref(), Some("apple-music:ABC123"));
    }

    #[test]
    fn closed_stopped_and_missing_data() {
        let now = Instant::now();
        assert!(parse_output("closed", &[MacPlayer::Spotify], now).is_empty());
        assert!(
            parse_output(&record(&["error", "-600"]), &[MacPlayer::Spotify], now).is_empty(),
            "quit while reading"
        );
        let stopped = parse_output("stopped", &[MacPlayer::Music], now);
        assert!(
            matches!(&stopped[0], Reading::Snapshot(s) if s.status == PlaybackStatus::Stopped && s.track.is_none())
        );
        let radio = parse_output(&record(&["nodata", "playing"]), &[MacPlayer::Music], now);
        assert!(matches!(
            &radio[0],
            Reading::Unavailable {
                problem: Problem::NoTrackData,
                playing: Some(true),
                ..
            }
        ));
        let garbage = parse_output("", &[MacPlayer::Spotify, MacPlayer::Music], now);
        assert_eq!(
            garbage.len(),
            2,
            "a missing answer is a read failure for each player"
        );
    }
}
