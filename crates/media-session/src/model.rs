//! What a player reports, in one shape for every backend.

use std::fmt;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// The streaming services the app knows by name. Everything else is [`Service::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Service {
    Spotify,
    AppleMusic,
    AmazonMusic,
    Other,
}

impl Service {
    /// The services the overlay follows in automatic mode, in tie-break order.
    pub const KNOWN: [Service; 3] = [Service::Spotify, Service::AppleMusic, Service::AmazonMusic];

    pub fn title(self) -> &'static str {
        match self {
            Service::Spotify => "Spotify",
            Service::AppleMusic => "Apple Music",
            Service::AmazonMusic => "Amazon Music",
            Service::Other => "Other player",
        }
    }
}

/// Identifies one player for as long as it stays open, for example
/// `mpris:org.mpris.MediaPlayer2.spotify` or `smtc:Spotify.exe`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerId(pub String);

impl PlayerId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PlayerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerInfo {
    pub id: PlayerId,
    /// Shown to the listener, for example "Spotify" or "Firefox".
    pub name: String,
    pub service: Service,
    /// True for a web player inside a browser.
    pub web: bool,
}

/// What a player lets other apps control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub play_pause: bool,
    pub next: bool,
    pub previous: bool,
    pub seek: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Stable across players and restarts; keys per-song settings such as the
    /// timing correction. See [`crate::normalize::track_key`].
    pub key: String,
    /// The player's own identifier, such as `spotify:track:…`, when it has one.
    pub source_id: Option<String>,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
    pub artwork_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

/// One reading of a player that could be read.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub player: PlayerInfo,
    pub status: PlaybackStatus,
    pub track: Option<Track>,
    /// Seconds into the track at `sampled_at`, when the player reports it.
    pub position: Option<f64>,
    pub sampled_at: Instant,
    /// Playback speed; 1.0 is normal.
    pub rate: f64,
    pub capabilities: Capabilities,
}

impl PlayerSnapshot {
    /// The position extrapolated to `now`, using the reading's own timestamp.
    pub fn position_at(&self, now: Instant) -> Option<f64> {
        let position = self.position?;
        let clock =
            lyrics_core::PlaybackClock::new(position, 0.0, self.status == PlaybackStatus::Playing)
                .with_rate(self.rate);
        let elapsed = now.saturating_duration_since(self.sampled_at).as_secs_f64();
        let duration = self
            .track
            .as_ref()
            .and_then(|track| track.duration)
            .unwrap_or(0.0);
        Some(clock.position_at(elapsed, duration))
    }
}

/// Why an open player could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Problem {
    /// macOS Automation permission is missing for this player.
    PermissionDenied,
    /// The player is open but reports no usable track, for example a radio stream.
    NoTrackData,
    /// The player did not answer as expected.
    ReadFailed { detail: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Reading {
    Snapshot(PlayerSnapshot),
    Unavailable {
        player: PlayerInfo,
        problem: Problem,
        /// Whether the player said it is playing, when that much was readable.
        playing: Option<bool>,
    },
}

impl Reading {
    pub fn player(&self) -> &PlayerInfo {
        match self {
            Reading::Snapshot(snapshot) => &snapshot.player,
            Reading::Unavailable { player, .. } => player,
        }
    }
}

/// Transport commands for the mini player and the sleep timer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Command {
    PlayPause,
    /// Pauses a playing player. Unlike [`Command::PlayPause`] it never starts
    /// playback, so the sleep timer can send it without knowing the state.
    Pause,
    Next,
    Previous,
    /// Jump to this many seconds into the track.
    SeekTo {
        position: f64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn snapshot(
        status: PlaybackStatus,
        position: Option<f64>,
        duration: Option<f64>,
    ) -> PlayerSnapshot {
        PlayerSnapshot {
            player: PlayerInfo {
                id: PlayerId::new("test"),
                name: "Test".into(),
                service: Service::Other,
                web: false,
            },
            status,
            track: Some(Track {
                key: "k".into(),
                source_id: None,
                title: "T".into(),
                artist: "A".into(),
                album: None,
                duration,
                artwork_url: None,
            }),
            position,
            sampled_at: Instant::now(),
            rate: 1.0,
            capabilities: Capabilities::default(),
        }
    }

    #[test]
    fn extrapolates_from_the_reading_time() {
        let reading = snapshot(PlaybackStatus::Playing, Some(10.0), Some(100.0));
        let later = reading.sampled_at + Duration::from_millis(1_500);
        assert!((reading.position_at(later).expect("has a position") - 11.5).abs() < 1e-9);
    }

    #[test]
    fn paused_and_unknown_positions_do_not_move() {
        let paused = snapshot(PlaybackStatus::Paused, Some(10.0), Some(100.0));
        assert_eq!(
            paused.position_at(paused.sampled_at + Duration::from_secs(5)),
            Some(10.0)
        );
        let unknown = snapshot(PlaybackStatus::Playing, None, Some(100.0));
        assert_eq!(unknown.position_at(Instant::now()), None);
    }

    #[test]
    fn stops_at_the_end_of_the_track() {
        let reading = snapshot(PlaybackStatus::Playing, Some(99.0), Some(100.0));
        assert_eq!(
            reading.position_at(reading.sampled_at + Duration::from_secs(5)),
            Some(100.0)
        );
    }
}
