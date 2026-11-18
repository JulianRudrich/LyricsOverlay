//! Singline: synced lyrics above your other windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autohide;
mod commands;
mod history;
mod i18n;
mod own_lyrics;
mod playback;
mod settings;
mod sleep;
mod tray;
mod windows;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use lyrics_providers::{LrclibClient, LyricsService, Store};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use playback::{LyricsView, PlaybackView, Request};
use settings::Settings;

pub struct AppState {
    pub demo: bool,
    /// Set by SINGLINE_DEBUG; the overlay then reports each line it shows.
    pub debug: bool,
    settings: Mutex<Settings>,
    /// `None` in the demo, which never changes the listener's files.
    settings_path: Option<PathBuf>,
    settings_dirty: AtomicBool,
    store: Arc<Mutex<Store>>,
    pub lyrics: Arc<LyricsService>,
    pub playback: Mutex<PlaybackView>,
    pub lyrics_view: Mutex<LyricsView>,
    pub lyrics_generation: AtomicU64,
    requests: Mutex<Sender<Request>>,
    pub hotkey_error: AtomicBool,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn request(&self, request: Request) {
        let _ = self
            .requests
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(request);
    }

    /// Changes the settings without side effects, for values the windows report
    /// themselves, such as the overlay position.
    pub fn update_settings_quietly(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.lock().unwrap_or_else(|p| p.into_inner());
        change(&mut settings);
        *settings = settings.clone().sanitized();
        self.settings_dirty.store(true, Ordering::SeqCst);
    }

    /// Changes the settings and applies what changed to the windows, the tray,
    /// the player loop and the web interface.
    pub fn update_settings(&self, app: &AppHandle, change: impl FnOnce(&mut Settings)) {
        let (old, new) = {
            let mut settings = self.settings.lock().unwrap_or_else(|p| p.into_inner());
            let old = settings.clone();
            change(&mut settings);
            *settings = settings.clone().sanitized();
            (old, settings.clone())
        };
        if old == new {
            return;
        }
        self.settings_dirty.store(true, Ordering::SeqCst);

        if old.overlay_size() != new.overlay_size() {
            windows::apply_size(app, &new);
        }
        if old.click_through != new.click_through {
            windows::set_click_through(app, new.click_through);
        }
        if old.overlay_visible != new.overlay_visible || old.auto_hide != new.auto_hide {
            autohide::settings_changed(app, &old, &new);
        }
        if (old.source, old.include_other_players) != (new.source, new.include_other_players) {
            self.request(Request::Preference {
                source: new.source,
                include_other: new.include_other_players,
            });
        }
        if old.lookup_online != new.lookup_online {
            self.request(Request::ReloadLyrics { refresh: false });
        }
        if old.hotkey != new.hotkey {
            register_hotkey(app, self, &new.hotkey);
        }
        if old.language != new.language {
            let title = i18n::Texts::new(i18n::resolve(new.language)).get("settings.title");
            windows::set_settings_title(app, &title);
            let title = i18n::Texts::new(i18n::resolve(new.language)).get("editor.title");
            windows::set_editor_title(app, &title);
        }
        let tray_changed = old.click_through != new.click_through
            || old.overlay_visible != new.overlay_visible
            || old.source != new.source
            || old.language != new.language;
        if tray_changed {
            tray::refresh(app);
        }
        let _ = tauri::Emitter::emit(app, "settings", &new);
        if old.language != new.language || old.hotkey != new.hotkey {
            commands::emit_app_info(app, self);
        }
    }

    pub fn save_settings_now(&self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        self.settings_dirty.store(false, Ordering::SeqCst);
        if let Err(error) = self.settings().save(path) {
            eprintln!("could not save the settings: {error}");
        }
    }
}

fn register_hotkey(app: &AppHandle, state: &AppState, hotkey: &str) {
    let shortcuts = app.global_shortcut();
    let _ = shortcuts.unregister_all();
    let failed = match shortcuts.register(hotkey) {
        Ok(()) => false,
        Err(error) => {
            eprintln!("could not register the shortcut {hotkey}: {error}");
            true
        }
    };
    state.hotkey_error.store(failed, Ordering::SeqCst);
}

fn user_agent(app: &AppHandle) -> String {
    format!(
        "Singline/{} (https://github.com/JulianRudrich/LyricsOverlay)",
        app.package_info().version
    )
}

fn setup(app: &mut tauri::App, demo: bool) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let override_dir = std::env::var_os("SINGLINE_DATA_DIR").map(PathBuf::from);
    let config_dir = override_dir
        .clone()
        .or_else(|| handle.path().app_config_dir().ok());
    let data_dir = override_dir.or_else(|| handle.path().app_data_dir().ok());

    let settings_path = config_dir.map(|dir| dir.join("settings.json"));
    let settings = settings_path
        .as_deref()
        .map(Settings::load)
        .unwrap_or_default();
    let store = match (&data_dir, demo) {
        (Some(dir), false) => Store::open(&dir.join("singline.sqlite")).or_else(|error| {
            eprintln!("could not open the song database, keeping data in memory: {error}");
            Store::open_in_memory()
        })?,
        _ => Store::open_in_memory()?,
    };
    let store = Arc::new(Mutex::new(store));
    let lrclib_url = std::env::var("SINGLINE_LRCLIB_URL")
        .unwrap_or_else(|_| lyrics_providers::lrclib::DEFAULT_BASE_URL.to_owned());
    let client = LrclibClient::with_base_url(&lrclib_url, &user_agent(&handle));
    let (sender, receiver) = mpsc::channel();

    app.manage(AppState {
        demo,
        debug: std::env::var_os("SINGLINE_DEBUG").is_some(),
        settings: Mutex::new(settings.clone()),
        settings_path: if demo { None } else { settings_path },
        settings_dirty: AtomicBool::new(false),
        store: Arc::clone(&store),
        lyrics: Arc::new(LyricsService::new(client, store)),
        playback: Mutex::new(PlaybackView::idle()),
        lyrics_view: Mutex::new(LyricsView::idle()),
        lyrics_generation: AtomicU64::new(0),
        requests: Mutex::new(sender),
        hotkey_error: AtomicBool::new(false),
    });
    app.manage(own_lyrics::EditorSession::default());
    app.manage(autohide::AutoHideState::new(settings.overlay_visible));
    app.manage(sleep::SleepState::default());
    app.manage(history::History::default());

    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    windows::create_overlay(&handle, &settings)?;
    tray::create(&handle)?;
    register_hotkey(&handle, &handle.state::<AppState>(), &settings.hotkey);
    playback::spawn(handle.clone(), demo, receiver);

    let saver = handle.clone();
    thread::Builder::new()
        .name("settings".into())
        .spawn(move || loop {
            thread::sleep(Duration::from_millis(750));
            let state = saver.state::<AppState>();
            if state.settings_dirty.load(Ordering::SeqCst) {
                state.save_settings_now();
            }
        })?;
    Ok(())
}

fn main() {
    let demo = std::env::args().any(|argument| argument == "--demo");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            |app, _arguments, _directory| windows::reopen(app),
        ))
        // Opens the links in the settings in the browser.
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        windows::toggle_overlay(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::update_settings,
            commands::adjust_song_offset,
            commands::set_song_offset,
            commands::reload_lyrics,
            own_lyrics::open_editor,
            own_lyrics::close_editor,
            own_lyrics::editor_state,
            own_lyrics::stamp_line,
            own_lyrics::save_local_lyrics,
            own_lyrics::import_lyrics_file,
            own_lyrics::remove_local_lyrics,
            commands::player_command,
            commands::open_settings,
            commands::move_overlay,
            commands::set_launch_at_login,
            commands::report_line,
            history::listening_stats,
            history::clear_history,
        ])
        .setup(move |app| setup(app, demo))
        .build(tauri::generate_context!())
        .expect("Singline could not start")
        .run(|app, event| match event {
            // Closing the settings window must not quit the app.
            tauri::RunEvent::ExitRequested {
                api, code: None, ..
            } => api.prevent_exit(),
            tauri::RunEvent::Exit => {
                history::flush(app);
                app.state::<AppState>().save_settings_now();
            }
            _ => {}
        });
}
