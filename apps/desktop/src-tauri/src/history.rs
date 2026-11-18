//! The listening history: records every song heard while the app runs and
//! sums it up for the "Your music" window. It stays in the song database on
//! this device and is never sent anywhere.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lyrics_core::listening::{
    summarize, Heard, ListenSession, ListeningLog, ListeningRange, ListeningSummary, Song,
};
use lyrics_core::SyncLevel;
use media_session::{PlaybackStatus, PlayerId, Reading, Service};
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

/// How often an open "Your music" window hears about the song playing now.
const LIVE_INTERVAL: Duration = Duration::from_secs(10);

pub struct History {
    inner: Mutex<Inner>,
}

struct Inner {
    log: ListeningLog<PlayerId>,
    last_update: Instant,
}

impl Default for History {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                log: ListeningLog::new(),
                last_update: Instant::now(),
            }),
        }
    }
}

impl History {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// After every reading of the players. Songs from every player count, not only
/// the followed one; players outside the known services count only when the
/// listener lets the overlay follow them too, so videos in a browser stay out.
pub fn observe(app: &AppHandle, readings: &[Reading], followed: Option<&PlayerId>) {
    let Some(history) = app.try_state::<History>() else {
        return;
    };
    let state = app.state::<AppState>();
    let settings = state.settings();
    if !settings.keep_history {
        // Turning the history off drops the song in progress too.
        history.lock().log = ListeningLog::new();
        return;
    }
    let synced = synced_track(&state);
    let counted = |service: Service| service != Service::Other || settings.include_other_players;
    let heard: Vec<(PlayerId, Option<Heard<'_>>)> = readings
        .iter()
        .filter_map(|reading| match reading {
            Reading::Snapshot(snapshot) if counted(snapshot.player.service) => {
                let song = snapshot
                    .track
                    .as_ref()
                    .filter(|_| snapshot.status != PlaybackStatus::Stopped)
                    .map(|track| Song {
                        key: &track.key,
                        title: &track.title,
                        artist: &track.artist,
                        album: track.album.as_deref(),
                        service: service_name(snapshot.player.service),
                        duration: track.duration,
                    });
                let shows_synced = followed == Some(&snapshot.player.id)
                    && snapshot.position.is_some()
                    && song.is_some_and(|song| synced.as_deref() == Some(song.key));
                let heard = Heard {
                    song,
                    position: snapshot.position,
                    playing: snapshot.status == PlaybackStatus::Playing,
                    synced_lyrics: shows_synced,
                };
                Some((snapshot.player.id.clone(), Some(heard)))
            }
            // Open but unreadable for a moment: its song carries on.
            Reading::Unavailable { player, .. } if counted(player.service) => {
                Some((player.id.clone(), None))
            }
            _ => None,
        })
        .collect();

    let (finished, update) = {
        let mut inner = history.lock();
        let finished = inner.log.tick(heard, unix_now());
        let live = inner.log.playing() && inner.last_update.elapsed() >= LIVE_INTERVAL;
        let update = live || !finished.is_empty();
        if update {
            inner.last_update = Instant::now();
        }
        (finished, update)
    };
    save(app, &finished);
    if update {
        let _ = app.emit("history", ());
    }
}

/// Keeps the songs still playing when the app quits.
pub fn flush(app: &AppHandle) {
    let Some(history) = app.try_state::<History>() else {
        return;
    };
    let finished = history.lock().log.finish_all();
    if app.state::<AppState>().settings().keep_history {
        save(app, &finished);
    }
}

fn save(app: &AppHandle, sessions: &[ListenSession]) {
    if sessions.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let store = state.store();
    for session in sessions {
        if let Err(error) = store.add_listen(session) {
            eprintln!("could not save a song to the listening history: {error}");
        }
    }
}

/// The song whose synced lyrics the overlay shows now.
fn synced_track(state: &AppState) -> Option<String> {
    let view = state.lyrics_view.lock().unwrap_or_else(|p| p.into_inner());
    let lyrics = view.lyrics.as_ref()?;
    (view.status == "ready" && lyrics.sync != SyncLevel::Unsynced && lyrics.has_text())
        .then(|| view.track_key.clone())
        .flatten()
}

/// The service as the web interface names it.
fn service_name(service: Service) -> &'static str {
    match service {
        Service::Spotify => "spotify",
        Service::AppleMusic => "appleMusic",
        Service::AmazonMusic => "amazonMusic",
        Service::YouTubeMusic => "youTubeMusic",
        Service::Other => "other",
    }
}

fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64())
}

/// Everything "Your music" shows for `range`, counting the songs playing now.
#[tauri::command]
pub async fn listening_stats(
    app: AppHandle,
    range: ListeningRange,
) -> Result<ListeningSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut sessions = app
            .state::<AppState>()
            .store()
            .listens(None)
            .map_err(|error| error.to_string())?;
        if let Some(history) = app.try_state::<History>() {
            sessions.extend(history.lock().log.in_progress());
        }
        Ok(summarize(&sessions, range, &chrono::Local::now()))
    })
    .await
    .map_err(|error| error.to_string())?
}

/// Deletes the whole listening history, including the song playing now.
#[tauri::command]
pub async fn clear_history(app: AppHandle) -> Result<(), String> {
    let cleared = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(history) = cleared.try_state::<History>() {
            history.lock().log = ListeningLog::new();
        }
        cleared
            .state::<AppState>()
            .store()
            .clear_listens()
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    let _ = app.emit("history", ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn services_are_named_like_the_interface_names_them() {
        for service in [
            Service::Spotify,
            Service::AppleMusic,
            Service::AmazonMusic,
            Service::Other,
        ] {
            assert_eq!(
                serde_json::to_value(service).expect("serializes"),
                service_name(service)
            );
        }
    }
}
