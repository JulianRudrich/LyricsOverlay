//! A built-in player for previews and screenshots: plays a short German song on a
//! loop, with word timing so the word-by-word highlight can be seen.

use std::time::{Duration, Instant};

use crate::backend::{Backend, CommandError};
use crate::model::{
    Capabilities, Command, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Reading, Service,
    Track,
};

pub const DEMO_PLAYER_ID: &str = "demo";

const DEMO_TITLE: &str = "Nachthimmel";
const DEMO_ARTIST: &str = "Singline";
const DEMO_DURATION: f64 = 24.0;

/// The demo song in enhanced LRC.
pub const DEMO_LRC: &str = "\
[00:00.00]<00:00.00>Leise <00:00.70>zieht <00:01.30>die <00:01.80>Nacht <00:02.40>vor<00:02.90>bei<00:03.60>
[00:04.00]<00:04.00>Über <00:04.70>Dächern, <00:05.60>sacht <00:06.30>und <00:06.80>frei<00:07.60>
[00:08.00]<00:08.00>In <00:08.40>den <00:08.80>Fenstern <00:09.70>brennt <00:10.40>noch <00:10.90>Licht<00:11.70>
[00:12.00]<00:12.00>Und <00:12.50>der <00:12.90>Himmel <00:13.80>ändert <00:14.60>sich<00:15.40>
[00:16.00]
[00:20.00]<00:20.00>Bis <00:20.50>der <00:20.90>erste <00:21.60>Morgen <00:22.40>spricht<00:23.30>
";

/// Plays [`DEMO_LRC`] in a loop and obeys the transport commands.
#[derive(Debug, Clone)]
pub struct DemoBackend {
    started: Instant,
    offset: f64,
    paused_at: Option<f64>,
}

impl Default for DemoBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoBackend {
    pub fn new() -> Self {
        Self {
            started: Instant::now(),
            offset: 0.0,
            paused_at: None,
        }
    }

    fn position(&self, now: Instant) -> f64 {
        self.paused_at.unwrap_or_else(|| {
            (self.offset + now.duration_since(self.started).as_secs_f64()).rem_euclid(DEMO_DURATION)
        })
    }

    fn restart_at(&mut self, position: f64) {
        self.started = Instant::now();
        self.offset = position.clamp(0.0, DEMO_DURATION);
        if self.paused_at.is_some() {
            self.paused_at = Some(self.offset);
        }
    }

    pub fn player_info() -> PlayerInfo {
        PlayerInfo {
            id: PlayerId::new(DEMO_PLAYER_ID),
            name: "Demo".into(),
            service: Service::Spotify,
            web: false,
        }
    }

    pub fn track() -> Track {
        Track {
            key: crate::normalize::track_key(DEMO_TITLE, DEMO_ARTIST, None),
            source_id: Some(DEMO_PLAYER_ID.into()),
            title: DEMO_TITLE.into(),
            artist: DEMO_ARTIST.into(),
            album: None,
            duration: Some(DEMO_DURATION),
            artwork_url: None,
        }
    }
}

impl Backend for DemoBackend {
    fn name(&self) -> &'static str {
        "demo"
    }

    fn poll(&mut self) -> Vec<Reading> {
        let now = Instant::now();
        vec![Reading::Snapshot(PlayerSnapshot {
            player: Self::player_info(),
            status: if self.paused_at.is_some() {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Playing
            },
            track: Some(Self::track()),
            position: Some(self.position(now)),
            sampled_at: now,
            rate: 1.0,
            capabilities: Capabilities {
                play_pause: true,
                next: true,
                previous: true,
                seek: true,
            },
        })]
    }

    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
        if player.as_str() != DEMO_PLAYER_ID {
            return Err(CommandError::PlayerGone);
        }
        match command {
            Command::PlayPause => {
                self.paused_at = match self.paused_at {
                    Some(position) => {
                        self.started = Instant::now();
                        self.offset = position;
                        None
                    }
                    None => Some(self.position(Instant::now())),
                };
            }
            Command::Next | Command::Previous => self.restart_at(0.0),
            Command::SeekTo { position } => self.restart_at(position),
        }
        Ok(())
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(backend: &mut DemoBackend) -> PlayerSnapshot {
        match backend.poll().pop() {
            Some(Reading::Snapshot(snapshot)) => snapshot,
            other => panic!("unexpected reading {other:?}"),
        }
    }

    #[test]
    fn demo_lyrics_have_word_timing() {
        let lines = lyrics_core::lrc::parse(DEMO_LRC);
        assert_eq!(lines.len(), 6);
        assert!(lines
            .iter()
            .filter(|line| !line.is_break())
            .all(|line| !line.words.is_empty()));
        assert!(lines.last().is_some_and(|line| line.time < DEMO_DURATION));
    }

    #[test]
    fn pause_seek_and_resume() {
        let mut demo = DemoBackend::new();
        let id = PlayerId::new(DEMO_PLAYER_ID);
        demo.send(&id, Command::PlayPause)
            .expect("demo accepts commands");
        assert_eq!(snapshot(&mut demo).status, PlaybackStatus::Paused);
        demo.send(&id, Command::SeekTo { position: 12.5 })
            .expect("demo accepts commands");
        assert_eq!(snapshot(&mut demo).position, Some(12.5));
        demo.send(&id, Command::PlayPause)
            .expect("demo accepts commands");
        let playing = snapshot(&mut demo);
        assert_eq!(playing.status, PlaybackStatus::Playing);
        assert!(playing.position.is_some_and(|p| (12.5..13.5).contains(&p)));
        assert_eq!(
            demo.send(&PlayerId::new("other"), Command::Next),
            Err(CommandError::PlayerGone)
        );
    }
}
