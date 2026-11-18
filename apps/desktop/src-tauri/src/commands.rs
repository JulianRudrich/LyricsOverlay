//! The commands the web interface calls.

use std::sync::atomic::Ordering;

use media_session::Command;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;

use crate::hotkey::HotkeyError;
use crate::i18n::{resolve, Lang};
use crate::playback::{LyricsView, PlaybackView, Request};
use crate::settings::{Corner, Settings, DEFAULT_HOTKEY};
use crate::{windows, AppState};

/// The largest correction for one song, in seconds.
const MAX_SONG_OFFSET: f64 = 10.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub demo: bool,
    pub debug: bool,
    pub platform: &'static str,
    pub language: Lang,
    pub hotkey_error: bool,
    pub default_hotkey: &'static str,
    pub launch_at_login: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub playback: PlaybackView,
    pub lyrics: LyricsView,
    pub app: AppInfo,
}

pub fn app_info(app: &AppHandle, state: &AppState) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        demo: state.demo,
        debug: state.debug,
        platform: if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            "other"
        },
        language: resolve(state.settings().language),
        hotkey_error: state.hotkey_error.load(Ordering::SeqCst),
        default_hotkey: DEFAULT_HOTKEY,
        launch_at_login: app.autolaunch().is_enabled().unwrap_or(false),
    }
}

pub fn emit_app_info(app: &AppHandle, state: &AppState) {
    let _ = app.emit("app", app_info(app, state));
}

#[tauri::command]
pub fn snapshot(app: AppHandle, state: State<'_, AppState>) -> Snapshot {
    Snapshot {
        settings: state.settings(),
        playback: state
            .playback
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone(),
        lyrics: state
            .lyrics_view
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone(),
        app: app_info(&app, &state),
    }
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    mut patch: serde_json::Value,
) -> Result<Settings, String> {
    // The shortcut changes through `set_hotkey`, which first checks it can be registered.
    if let Some(changes) = patch.as_object_mut() {
        changes.remove("hotkey");
    }
    let merged = state.settings().merged(&patch)?;
    state.update_settings(&app, move |settings| *settings = merged);
    Ok(state.settings())
}

#[tauri::command]
pub fn adjust_song_offset(app: AppHandle, state: State<'_, AppState>, delta: f64) -> f64 {
    let current = state
        .lyrics_view
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .song_offset;
    set_song_offset(app, state, current + delta)
}

#[tauri::command]
pub fn set_song_offset(app: AppHandle, state: State<'_, AppState>, value: f64) -> f64 {
    let mut view = state
        .lyrics_view
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let Some(track_key) = view.track_key.clone() else {
        return 0.0;
    };
    let offset = if value.is_finite() {
        (value.clamp(-MAX_SONG_OFFSET, MAX_SONG_OFFSET) * 100.0).round() / 100.0
    } else {
        0.0
    };
    if let Err(error) = state.store().set_timing_offset(&track_key, offset) {
        eprintln!("could not save the timing correction: {error}");
        return view.song_offset;
    }
    view.song_offset = offset;
    crate::playback::publish(&app, view);
    offset
}

#[tauri::command]
pub fn reload_lyrics(state: State<'_, AppState>) {
    state.request(Request::ReloadLyrics { refresh: true });
}

#[tauri::command]
pub fn player_command(state: State<'_, AppState>, command: Command) {
    state.request(Request::Command(command));
}

/// Async, because building a window inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    windows::open_settings(&app);
}

/// Changes the shortcut that shows and hides the overlay. When another app
/// has the new one, the old one stays. Async, as registering waits for the
/// main thread.
#[tauri::command]
pub async fn set_hotkey(app: AppHandle, hotkey: String) -> Result<Settings, HotkeyError> {
    let state = app.state::<AppState>();
    crate::hotkey::change(&app, &state, &hotkey)?;
    Ok(state.settings())
}

#[tauri::command]
pub fn move_overlay(app: AppHandle, corner: Corner) {
    windows::move_to_corner(&app, corner);
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> bool {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    if let Err(error) = result {
        eprintln!("could not change launch at login: {error}");
    }
    launcher.is_enabled().unwrap_or(false)
}

/// Prints the line the overlay shows, for automated end-to-end checks.
#[tauri::command]
pub fn report_line(state: State<'_, AppState>, position: f64, text: String) {
    if state.debug {
        eprintln!("overlay-line {position:.2} {text}");
    }
}
