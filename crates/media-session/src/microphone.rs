//! The player for Sing to find: the song someone sings into the microphone.
//!
//! The app recognises the song and the position from what it hears and hands
//! them to a [`MicrophoneFeed`]. [`MicrophoneBackend`] reports them like any
//! other player, so the overlay, the word highlight and the listening history
//! work unchanged. While the feed is on, [`crate::Engine`] follows the
//! microphone and nothing else.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::backend::{Backend, CommandError};
use crate::model::{
    Capabilities, Command, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Reading, Service,
    Track,
};

pub const MICROPHONE_PLAYER_ID: &str = "microphone";

#[derive(Debug, Clone)]
struct Song {
    track: Track,
    /// Seconds into the track at `at`.
    position: f64,
    at: Instant,
    paused: bool,
}

impl Song {
    fn position_at(&self, now: Instant) -> f64 {
        let elapsed = if self.paused {
            0.0
        } else {
            now.saturating_duration_since(self.at).as_secs_f64()
        };
        let position = (self.position + elapsed).max(0.0);
        match self.track.duration {
            Some(duration) if duration > 0.0 => position.min(duration),
            _ => position,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    active: bool,
    song: Option<Song>,
    /// Counts changes made from the player side, such as a seek.
    moves: u64,
}

/// What Sing to find follows right now. Cheap to clone; all clones share it.
#[derive(Debug, Clone, Default)]
pub struct MicrophoneFeed {
    state: Arc<Mutex<State>>,
}

impl MicrophoneFeed {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Listening starts: the microphone takes over, before a song is found.
    pub fn start(&self) {
        let mut state = self.lock();
        state.active = true;
        state.song = None;
    }

    /// Listening ends, so the other players are followed again.
    pub fn stop(&self) {
        let mut state = self.lock();
        state.active = false;
        state.song = None;
    }

    pub fn is_active(&self) -> bool {
        self.lock().active
    }

    /// The song was found, or its position corrected: `position` seconds at `at`.
    /// Ignored while listening is off.
    pub fn follow(&self, track: Track, position: f64, at: Instant) {
        let mut state = self.lock();
        if !state.active {
            return;
        }
        let paused = state
            .song
            .as_ref()
            .is_some_and(|song| song.paused && song.track.key == track.key);
        state.song = Some(Song {
            track,
            position,
            at,
            paused,
        });
    }

    /// Back to listening without a song, to search again.
    pub fn clear_song(&self) {
        self.lock().song = None;
    }

    pub fn track(&self) -> Option<Track> {
        self.lock().song.as_ref().map(|song| song.track.clone())
    }

    /// The followed song's position at `now`.
    pub fn position_at(&self, now: Instant) -> Option<f64> {
        self.lock().song.as_ref().map(|song| song.position_at(now))
    }

    pub fn is_paused(&self) -> bool {
        self.lock().song.as_ref().is_some_and(|song| song.paused)
    }

    /// Changes whenever the listener pauses, resumes or seeks, so the follower
    /// knows its own timing was overruled.
    pub fn moves(&self) -> u64 {
        self.lock().moves
    }

    fn command(&self, command: Command) -> Result<(), CommandError> {
        let mut state = self.lock();
        let now = Instant::now();
        let song = state.song.as_mut().ok_or(CommandError::Unsupported)?;
        let position = match command {
            Command::PlayPause => {
                let position = song.position_at(now);
                song.paused = !song.paused;
                position
            }
            Command::Pause => {
                let position = song.position_at(now);
                song.paused = true;
                position
            }
            Command::SeekTo { position } => position.max(0.0),
            Command::Next | Command::Previous => return Err(CommandError::Unsupported),
        };
        song.position = position;
        song.at = now;
        state.moves += 1;
        Ok(())
    }
}

/// Reports the song of a [`MicrophoneFeed`] as the player "Microphone".
#[derive(Debug, Clone)]
pub struct MicrophoneBackend {
    feed: MicrophoneFeed,
}

impl MicrophoneBackend {
    pub fn new(feed: MicrophoneFeed) -> Self {
        Self { feed }
    }

    pub fn player_info() -> PlayerInfo {
        PlayerInfo {
            id: PlayerId::new(MICROPHONE_PLAYER_ID),
            name: "Microphone".into(),
            service: Service::Microphone,
            web: false,
        }
    }
}

impl Backend for MicrophoneBackend {
    fn name(&self) -> &'static str {
        "microphone"
    }

    fn poll(&mut self) -> Vec<Reading> {
        let state = self.feed.lock();
        if !state.active {
            return Vec::new();
        }
        let now = Instant::now();
        let song = state.song.as_ref();
        vec![Reading::Snapshot(PlayerSnapshot {
            player: Self::player_info(),
            status: if song.is_some_and(|song| song.paused) {
                PlaybackStatus::Paused
            } else {
                PlaybackStatus::Playing
            },
            track: song.map(|song| song.track.clone()),
            position: song.map(|song| song.position_at(now)),
            sampled_at: now,
            rate: 1.0,
            capabilities: Capabilities {
                play_pause: song.is_some(),
                next: false,
                previous: false,
                seek: song.is_some(),
            },
        })]
    }

    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
        if player.as_str() != MICROPHONE_PLAYER_ID {
            return Err(CommandError::PlayerGone);
        }
        self.feed.command(command)
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str, duration: Option<f64>) -> Track {
        Track {
            key: format!("key-{title}"),
            source_id: None,
            title: title.into(),
            artist: "Artist".into(),
            album: None,
            duration,
            artwork_url: None,
        }
    }

    fn snapshot(backend: &mut MicrophoneBackend) -> Option<PlayerSnapshot> {
        match backend.poll().pop() {
            Some(Reading::Snapshot(snapshot)) => Some(snapshot),
            None => None,
            other => panic!("unexpected reading {other:?}"),
        }
    }

    #[test]
    fn reports_nothing_until_listening_starts() {
        let feed = MicrophoneFeed::new();
        let mut backend = MicrophoneBackend::new(feed.clone());
        assert!(snapshot(&mut backend).is_none());
        feed.follow(track("Early", None), 1.0, Instant::now());
        assert!(feed.track().is_none(), "a song needs listening to be on");

        feed.start();
        let listening = snapshot(&mut backend).expect("listening");
        assert_eq!(listening.player.service, Service::Microphone);
        assert_eq!(listening.status, PlaybackStatus::Playing);
        assert!(listening.track.is_none() && listening.position.is_none());
        assert!(!listening.capabilities.seek);

        feed.stop();
        assert!(snapshot(&mut backend).is_none());
    }

    #[test]
    fn the_found_song_runs_on_from_where_it_was_sung() {
        let feed = MicrophoneFeed::new();
        let mut backend = MicrophoneBackend::new(feed.clone());
        feed.start();
        let sung_at = Instant::now() - Duration::from_millis(1_500);
        feed.follow(track("Found", Some(200.0)), 12.0, sung_at);
        let following = snapshot(&mut backend).expect("following");
        assert_eq!(following.track.map(|t| t.title).as_deref(), Some("Found"));
        let position = following.position.expect("has a position");
        assert!((13.5..14.0).contains(&position), "{position}");
        assert!(following.capabilities.seek && following.capabilities.play_pause);
        assert!(!following.capabilities.next);

        feed.clear_song();
        assert!(snapshot(&mut backend)
            .expect("still listening")
            .track
            .is_none());
    }

    #[test]
    fn pause_and_seek_are_moves_of_the_listener() {
        let feed = MicrophoneFeed::new();
        let mut backend = MicrophoneBackend::new(feed.clone());
        let id = PlayerId::new(MICROPHONE_PLAYER_ID);
        feed.start();
        assert_eq!(
            backend.send(&id, Command::PlayPause),
            Err(CommandError::Unsupported),
            "nothing to pause before a song is found"
        );
        feed.follow(track("Song", Some(100.0)), 10.0, Instant::now());
        let moves = feed.moves();
        backend.send(&id, Command::Pause).expect("pauses");
        assert!(feed.is_paused());
        assert_eq!(
            snapshot(&mut backend).map(|s| s.status),
            Some(PlaybackStatus::Paused)
        );
        backend
            .send(&id, Command::SeekTo { position: 42.0 })
            .expect("seeks");
        assert_eq!(
            feed.position_at(Instant::now() + Duration::from_secs(3)),
            Some(42.0)
        );
        assert_eq!(feed.moves(), moves + 2);
        // A correction from the follower keeps the pause.
        feed.follow(track("Song", Some(100.0)), 43.0, Instant::now());
        assert!(feed.is_paused());
        backend.send(&id, Command::PlayPause).expect("resumes");
        assert!(!feed.is_paused());
        assert_eq!(
            backend.send(&id, Command::Next),
            Err(CommandError::Unsupported)
        );
        assert_eq!(
            backend.send(&PlayerId::new("spotify"), Command::PlayPause),
            Err(CommandError::PlayerGone)
        );
        feed.follow(track("Song", Some(100.0)), 120.0, Instant::now());
        assert_eq!(
            feed.position_at(Instant::now()),
            Some(100.0),
            "stops at the end"
        );
    }
}
