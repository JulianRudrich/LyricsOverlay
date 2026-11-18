//! A pretend MPRIS player for tests and the Linux end-to-end check. It plays one
//! track on a loop in real time and obeys play/pause, pause, seek and skip
//! commands.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

use crate::mpris::{BUS_PREFIX, OBJECT_PATH};

#[derive(Debug, Clone)]
pub struct FakeTrack {
    /// An object path such as `/com/spotify/track/demo`.
    pub track_id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: String,
    pub length: Duration,
}

#[derive(Debug)]
struct State {
    track: FakeTrack,
    playing: bool,
    base: Duration,
    since: Instant,
    commands: Vec<String>,
}

impl State {
    fn position(&self) -> Duration {
        let elapsed = if self.playing {
            self.since.elapsed()
        } else {
            Duration::ZERO
        };
        let length = self.track.length.as_secs_f64().max(0.001);
        Duration::from_secs_f64((self.base + elapsed).as_secs_f64() % length)
    }

    fn set_position(&mut self, position: Duration) {
        self.base = position.min(self.track.length);
        self.since = Instant::now();
    }
}

/// Keeps the pretend player on the bus while alive.
pub struct FakePlayer {
    _connection: Connection,
    state: Arc<Mutex<State>>,
}

impl FakePlayer {
    /// Registers `org.mpris.MediaPlayer2.<suffix>` on the bus at `address`, or on
    /// the session bus when `address` is `None`.
    pub fn spawn(
        address: Option<&str>,
        suffix: &str,
        identity: &str,
        track: FakeTrack,
        playing: bool,
    ) -> zbus::Result<Self> {
        let state = Arc::new(Mutex::new(State {
            track,
            playing,
            base: Duration::ZERO,
            since: Instant::now(),
            commands: Vec::new(),
        }));
        let builder = match address {
            Some(address) => zbus::blocking::connection::Builder::address(address)?,
            None => zbus::blocking::connection::Builder::session()?,
        };
        let connection = builder
            .name(format!("{BUS_PREFIX}{suffix}"))?
            .serve_at(
                OBJECT_PATH,
                Root {
                    identity: identity.to_owned(),
                },
            )?
            .serve_at(
                OBJECT_PATH,
                Player {
                    state: Arc::clone(&state),
                },
            )?
            .build()?;
        Ok(Self {
            _connection: connection,
            state,
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn position(&self) -> Duration {
        self.state().position()
    }

    pub fn is_playing(&self) -> bool {
        self.state().playing
    }

    pub fn seek_to(&self, position: Duration) {
        self.state().set_position(position);
    }

    /// The commands received so far, such as `PlayPause` or `SetPosition`.
    pub fn commands(&self) -> Vec<String> {
        self.state().commands.clone()
    }
}

struct Root {
    identity: String,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        self.identity.clone()
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }
}

struct Player {
    state: Arc<Mutex<State>>,
}

impl Player {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn owned(value: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(value).expect("plain values convert")
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        if self.state().playing {
            "Playing"
        } else {
            "Paused"
        }
        .to_owned()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        let state = self.state();
        let track = &state.track;
        let mut metadata = HashMap::new();
        if let Ok(path) = ObjectPath::try_from(track.track_id.as_str()) {
            metadata.insert("mpris:trackid".to_owned(), owned(Value::from(path)));
        }
        metadata.insert(
            "xesam:title".to_owned(),
            owned(Value::from(track.title.as_str())),
        );
        metadata.insert(
            "xesam:artist".to_owned(),
            owned(Value::from(track.artists.clone())),
        );
        metadata.insert(
            "xesam:album".to_owned(),
            owned(Value::from(track.album.as_str())),
        );
        metadata.insert(
            "mpris:length".to_owned(),
            owned(Value::from(track.length.as_micros() as i64)),
        );
        metadata
    }

    #[zbus(property)]
    fn position(&self) -> i64 {
        self.state().position().as_micros() as i64
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        true
    }

    fn play_pause(&self) {
        let mut state = self.state();
        let position = state.position();
        state.playing = !state.playing;
        state.set_position(position);
        state.commands.push("PlayPause".into());
    }

    fn pause(&self) {
        let mut state = self.state();
        let position = state.position();
        state.playing = false;
        state.set_position(position);
        state.commands.push("Pause".into());
    }

    fn next(&self) {
        let mut state = self.state();
        state.set_position(Duration::ZERO);
        state.commands.push("Next".into());
    }

    fn previous(&self) {
        let mut state = self.state();
        state.set_position(Duration::ZERO);
        state.commands.push("Previous".into());
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        let mut state = self.state();
        if track_id.as_str() == state.track.track_id && position >= 0 {
            state.set_position(Duration::from_micros(position as u64));
        }
        state.commands.push("SetPosition".into());
    }

    fn seek(&self, offset: i64) {
        let mut state = self.state();
        let target = (state.position().as_micros() as i64 + offset).max(0);
        state.set_position(Duration::from_micros(target as u64));
        state.commands.push("Seek".into());
    }
}
