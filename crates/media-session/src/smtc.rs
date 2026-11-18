//! Reading players on Windows through the System Media Transport Controls, the
//! same sessions the volume flyout shows.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as WindowsStatus,
};

use crate::backend::{Backend, CommandError};
use crate::model::{
    Capabilities, Command, PlaybackStatus, PlayerId, PlayerInfo, PlayerSnapshot, Reading, Service,
    Track,
};
use crate::normalize::{identify_smtc, split_artist_album, track_key};

/// 100-nanosecond ticks between 1601-01-01 and 1970-01-01.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const TICKS_PER_SECOND: f64 = 10_000_000.0;
/// A timeline update older than this is not used to move the position forward.
const MAX_EXTRAPOLATION: f64 = 6.0 * 60.0 * 60.0;

#[derive(Default)]
pub struct SmtcBackend {
    manager: Option<Manager>,
    last_error: Option<String>,
}

impl SmtcBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn manager(&mut self) -> windows::core::Result<Manager> {
        if let Some(manager) = &self.manager {
            return Ok(manager.clone());
        }
        let manager = Manager::RequestAsync()?.join()?;
        self.manager = Some(manager.clone());
        Ok(manager)
    }

    /// Every session with the player id it gets, in the order Windows lists them.
    fn sessions(&mut self) -> windows::core::Result<Vec<(PlayerId, Session)>> {
        let sessions = self.manager()?.GetSessions()?;
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut result = Vec::new();
        for index in 0..sessions.Size()? {
            let session = sessions.GetAt(index)?;
            let app_id = session.SourceAppUserModelId()?.to_string_lossy();
            let count = seen.entry(app_id.clone()).or_insert(0);
            *count += 1;
            let id = if *count == 1 {
                format!("smtc:{app_id}")
            } else {
                format!("smtc:{app_id}#{count}")
            };
            result.push((PlayerId::new(id), session));
        }
        Ok(result)
    }
}

impl Backend for SmtcBackend {
    fn name(&self) -> &'static str {
        "smtc"
    }

    fn poll(&mut self) -> Vec<Reading> {
        match self.sessions() {
            Ok(sessions) => {
                self.last_error = None;
                sessions
                    .into_iter()
                    .filter_map(|(id, session)| read_session(id, &session).ok())
                    .collect()
            }
            Err(error) => {
                self.manager = None;
                self.last_error = Some(format!("Media sessions: {error}"));
                Vec::new()
            }
        }
    }

    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
        let failed = |error: windows::core::Error| CommandError::Failed(error.to_string());
        let session = self
            .sessions()
            .map_err(failed)?
            .into_iter()
            .find(|(id, _)| id == player)
            .map(|(_, session)| session)
            .ok_or(CommandError::PlayerGone)?;
        let accepted = match command {
            Command::PlayPause => session.TryTogglePlayPauseAsync().and_then(|op| op.join()),
            Command::Pause => session.TryPauseAsync().and_then(|op| op.join()),
            Command::Next => session.TrySkipNextAsync().and_then(|op| op.join()),
            Command::Previous => session.TrySkipPreviousAsync().and_then(|op| op.join()),
            Command::SeekTo { position } => {
                let ticks = (position.max(0.0) * TICKS_PER_SECOND) as i64;
                session
                    .TryChangePlaybackPositionAsync(ticks)
                    .and_then(|op| op.join())
            }
        }
        .map_err(failed)?;
        if accepted {
            Ok(())
        } else {
            Err(CommandError::Unsupported)
        }
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    fn last_error(&self) -> Option<String> {
        self.last_error.clone()
    }
}

fn read_session(id: PlayerId, session: &Session) -> windows::core::Result<Reading> {
    let app_id = session.SourceAppUserModelId()?.to_string_lossy();
    let (service, name, web) = identify_smtc(&app_id);
    let info = session.GetPlaybackInfo()?;
    let status = match info.PlaybackStatus()? {
        WindowsStatus::Playing | WindowsStatus::Changing => PlaybackStatus::Playing,
        WindowsStatus::Paused => PlaybackStatus::Paused,
        _ => PlaybackStatus::Stopped,
    };
    let capabilities = info
        .Controls()
        .map(|controls| Capabilities {
            play_pause: controls.IsPlayPauseToggleEnabled().unwrap_or(false),
            next: controls.IsNextEnabled().unwrap_or(false),
            previous: controls.IsPreviousEnabled().unwrap_or(false),
            seek: controls.IsPlaybackPositionEnabled().unwrap_or(false),
        })
        .unwrap_or_default();
    let rate = info
        .PlaybackRate()
        .and_then(|rate| rate.Value())
        .ok()
        .filter(|rate| rate.is_finite() && *rate > 0.0)
        .unwrap_or(1.0);

    let properties = session.TryGetMediaPropertiesAsync()?.join()?;
    let title = properties
        .Title()
        .map(|t| t.to_string_lossy())
        .unwrap_or_default();
    let raw_artist = properties
        .Artist()
        .map(|a| a.to_string_lossy())
        .unwrap_or_default();
    let raw_album = properties
        .AlbumTitle()
        .map(|a| a.to_string_lossy())
        .unwrap_or_default();
    let (artist, album) = if service == Service::AppleMusic {
        split_artist_album(&raw_artist, Some(&raw_album))
    } else {
        let album = raw_album.trim();
        (
            raw_artist.trim().to_owned(),
            (!album.is_empty()).then(|| album.to_owned()),
        )
    };

    let timeline = session.GetTimelineProperties()?;
    let start = timeline.StartTime()?.Duration;
    let end = timeline.EndTime()?.Duration;
    let position_ticks = timeline.Position()?.Duration;
    let updated = timeline.LastUpdatedTime()?.UniversalTime;
    let sampled_at = Instant::now();
    let duration = (end > start).then(|| (end - start) as f64 / TICKS_PER_SECOND);
    let reports_position = capabilities.seek || position_ticks > 0 || duration.is_some();

    let title = title.trim().to_owned();
    let track = (!title.is_empty()).then(|| Track {
        key: track_key(&title, &artist, album.as_deref()),
        source_id: None,
        title,
        artist,
        album,
        duration,
        artwork_url: None,
    });
    let position = (track.is_some() && reports_position).then(|| {
        let reported = (position_ticks - start).max(0) as f64 / TICKS_PER_SECOND;
        reported + since_update(updated, status, rate)
    });

    Ok(Reading::Snapshot(PlayerSnapshot {
        player: PlayerInfo {
            id,
            name,
            service,
            web,
        },
        status,
        track,
        position,
        sampled_at,
        rate,
        capabilities,
    }))
}

/// Seconds of playback since the player last updated its timeline. Players such
/// as Spotify only update it on play, pause and seek.
fn since_update(updated: i64, status: PlaybackStatus, rate: f64) -> f64 {
    if status != PlaybackStatus::Playing || updated <= 0 {
        return 0.0;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| UNIX_EPOCH_TICKS + (elapsed.as_nanos() / 100) as i64)
        .unwrap_or(0);
    let elapsed = (now - updated) as f64 / TICKS_PER_SECOND;
    if (0.0..MAX_EXTRAPOLATION).contains(&elapsed) {
        elapsed * rate
    } else {
        0.0
    }
}
