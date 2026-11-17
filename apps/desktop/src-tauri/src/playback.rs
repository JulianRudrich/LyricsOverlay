//! The loop that reads the players, follows one of them and loads its lyrics.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use lyrics_core::{lrc, Lyrics};
use lyrics_providers::{LookupOptions, LyricsOutcome, Query};
use media_session::{
    default_backends, Backend, Capabilities, Command, CommandError, DemoBackend, Engine,
    NowPlaying, PlayerInfo, Problem, SourcePreference, Track, DEMO_LRC, DEMO_PLAYER_ID,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;

pub enum Request {
    Command(Command),
    Preference {
        source: SourcePreference,
        include_other: bool,
    },
    ReloadLyrics {
        refresh: bool,
    },
}

/// What the overlay shows about the player, as sent to the web interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackView {
    pub state: &'static str,
    pub player: Option<PlayerInfo>,
    pub problem: Option<Problem>,
    pub track: Option<Track>,
    pub playing: bool,
    /// Seconds into the track at the moment this view is sent.
    pub position: Option<f64>,
    pub rate: f64,
    pub capabilities: Capabilities,
}

impl PlaybackView {
    pub fn idle() -> Self {
        Self {
            state: "idle",
            player: None,
            problem: None,
            track: None,
            playing: false,
            position: None,
            rate: 1.0,
            capabilities: Capabilities::default(),
        }
    }

    fn new(now_playing: &NowPlaying, now: Instant) -> Self {
        match now_playing {
            NowPlaying::Idle => Self::idle(),
            NowPlaying::Problem { player, problem } => Self {
                state: "problem",
                player: Some(player.clone()),
                problem: Some(problem.clone()),
                ..Self::idle()
            },
            NowPlaying::Track(snapshot) => Self {
                state: "track",
                player: Some(snapshot.player.clone()),
                problem: None,
                track: snapshot.track.clone(),
                playing: snapshot.status == media_session::PlaybackStatus::Playing,
                position: snapshot.position_at(now),
                rate: snapshot.rate,
                capabilities: snapshot.capabilities,
            },
        }
    }
}

/// The lyrics of the followed track, as sent to the web interface.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsView {
    pub track_key: Option<String>,
    pub status: &'static str,
    pub origin: Option<&'static str>,
    pub lyrics: Option<Lyrics>,
    pub song_offset: f64,
    pub can_retry: bool,
}

impl LyricsView {
    pub fn idle() -> Self {
        Self {
            track_key: None,
            status: "idle",
            origin: None,
            lyrics: None,
            song_offset: 0.0,
            can_retry: false,
        }
    }

    fn loading(track_key: &str, song_offset: f64) -> Self {
        Self {
            track_key: Some(track_key.to_owned()),
            status: "loading",
            song_offset,
            ..Self::idle()
        }
    }

    fn ready(track_key: &str, lyrics: Lyrics, origin: &'static str, song_offset: f64) -> Self {
        Self {
            track_key: Some(track_key.to_owned()),
            status: "ready",
            origin: Some(origin),
            lyrics: Some(lyrics),
            song_offset,
            can_retry: false,
        }
    }

    fn from_outcome(track_key: &str, outcome: LyricsOutcome, song_offset: f64) -> Self {
        let can_retry = outcome.can_retry();
        let status = match outcome {
            LyricsOutcome::Local { lyrics } => {
                return Self::ready(track_key, lyrics, "local", song_offset)
            }
            LyricsOutcome::Online { lyrics } => {
                return Self::ready(track_key, lyrics, "online", song_offset)
            }
            LyricsOutcome::Instrumental => "instrumental",
            LyricsOutcome::NotFound => "notFound",
            LyricsOutcome::LocalOnly => "localOnly",
            LyricsOutcome::Offline => "offline",
            LyricsOutcome::RateLimited { .. } => "rateLimited",
            LyricsOutcome::Failed { message } => {
                eprintln!("lyrics lookup failed: {message}");
                "failed"
            }
        };
        Self {
            track_key: Some(track_key.to_owned()),
            status,
            song_offset,
            can_retry,
            ..Self::idle()
        }
    }
}

pub fn spawn(app: AppHandle, demo: bool, requests: Receiver<Request>) {
    let started = thread::Builder::new()
        .name("playback".into())
        .spawn(move || run(&app, demo, &requests));
    if let Err(error) = started {
        eprintln!("could not start reading players: {error}");
    }
}

fn run(app: &AppHandle, demo: bool, requests: &Receiver<Request>) {
    let (source, include_other) = {
        let state = app.state::<AppState>();
        let settings = state.settings();
        (settings.source, settings.include_other_players)
    };
    let mut engine = Engine::new(source, include_other);
    let mut backends: Vec<Box<dyn Backend>> = if demo {
        vec![Box::new(DemoBackend::new())]
    } else {
        default_backends()
    };
    let interval = backends
        .iter()
        .map(|backend| backend.poll_interval())
        .min()
        .unwrap_or(Duration::from_millis(500));
    let mut current_track: Option<Track> = None;

    loop {
        let readings = backends
            .iter_mut()
            .flat_map(|backend| backend.poll())
            .collect();
        let now_playing = engine.choose(readings);
        let view = PlaybackView::new(&now_playing, Instant::now());
        let track = view.track.clone();
        {
            let state = app.state::<AppState>();
            *state.playback.lock().unwrap_or_else(|p| p.into_inner()) = view.clone();
        }
        let _ = app.emit("playback", &view);
        if track.as_ref().map(|t| &t.key) != current_track.as_ref().map(|t| &t.key) {
            current_track = track;
            load_lyrics(app, current_track.clone(), demo, false);
        } else if track.is_some() {
            // Same song; keep the details fresh, such as a late duration.
            current_track = track;
        }

        let deadline = Instant::now() + interval;
        loop {
            match requests.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Request::Command(command)) => {
                    if let Some(player) = engine.current().cloned() {
                        for backend in backends.iter_mut() {
                            match backend.send(&player, command) {
                                Err(CommandError::PlayerGone) => continue,
                                Err(error) => eprintln!("{player}: {error}"),
                                Ok(()) => {}
                            }
                            break;
                        }
                    }
                    break;
                }
                Ok(Request::Preference {
                    source,
                    include_other,
                }) => {
                    engine.set_preference(source, include_other);
                    break;
                }
                Ok(Request::ReloadLyrics { refresh }) => {
                    load_lyrics(app, current_track.clone(), demo, refresh)
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

/// Loads the lyrics for `track` and sends them once they are found. A newer
/// request makes an older one's answer go unused.
pub fn load_lyrics(app: &AppHandle, track: Option<Track>, demo: bool, refresh: bool) {
    let state = app.state::<AppState>();
    let generation = state.lyrics_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let Some(track) = track else {
        publish(app, LyricsView::idle());
        return;
    };
    let song_offset = state.store().timing_offset(&track.key);

    if demo && track.source_id.as_deref() == Some(DEMO_PLAYER_ID) {
        let lyrics = Lyrics::timed(lrc::parse(DEMO_LRC));
        publish(
            app,
            LyricsView::ready(&track.key, lyrics, "demo", song_offset),
        );
        return;
    }

    publish(app, LyricsView::loading(&track.key, song_offset));
    let online = state.settings().lookup_online;
    let service = state.lyrics.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let query = Query {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration: track.duration.unwrap_or(0.0),
        };
        let outcome = service
            .lyrics_for(&track.key, &query, LookupOptions { online, refresh })
            .await;
        let state = app.state::<AppState>();
        if state.lyrics_generation.load(Ordering::SeqCst) == generation {
            publish(
                &app,
                LyricsView::from_outcome(&track.key, outcome, song_offset),
            );
        }
    });
}

pub fn publish(app: &AppHandle, view: LyricsView) {
    let state = app.state::<AppState>();
    *state.lyrics_view.lock().unwrap_or_else(|p| p.into_inner()) = view.clone();
    let _ = app.emit("lyrics", &view);
}
