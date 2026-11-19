//! On macOS Singline lives in the menu bar. While one of its windows is open
//! it also shows in the Dock, so it can be found in the app switcher and in
//! Force Quit, and the menu bar offers its app menu with Quit. Without a
//! window it leaves the Dock again, which keeps the overlay above apps in full
//! screen.

#[cfg(target_os = "macos")]
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};

use tauri::AppHandle;
#[cfg(target_os = "macos")]
use tauri::Manager;

#[cfg(target_os = "macos")]
use crate::windows;

/// The windows that bring Singline into the Dock. The overlay, the tray panel
/// and karaoke, which fills a display, do not.
#[cfg(target_os = "macos")]
const WINDOWS: [&str; 4] = [
    windows::SETTINGS,
    windows::STATS,
    windows::EDITOR,
    windows::SHUFFLE,
];

/// macOS ignores leaving the Dock within a second of entering it, so a window
/// closed sooner tries again after this.
#[cfg(target_os = "macos")]
const SETTLE: Duration = Duration::from_millis(1200);

/// The window the Dock icon was for gets the focus again after this, once
/// Singline is a regular app and the menu bar can show its menu.
#[cfg(target_os = "macos")]
const REFOCUS: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct Dock {
    /// Since when the icon shows, if it does.
    #[cfg(target_os = "macos")]
    shown: Mutex<Option<Instant>>,
}

/// Puts Singline into the Dock or takes it out, after the window `label`
/// opened or closed. `closing` names a window that is going away and may still
/// be listed.
#[cfg(target_os = "macos")]
pub fn update(app: &AppHandle, label: &str, closing: bool) {
    let Some(state) = app.try_state::<Dock>() else {
        return;
    };
    let open = WINDOWS
        .iter()
        .any(|name| (*name != label || !closing) && app.get_webview_window(name).is_some());
    let mut shown = state
        .shown
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match (open, *shown) {
        (true, None) => {
            if app.set_dock_visibility(true).is_ok() {
                *shown = Some(Instant::now());
                refocus(app, label);
            }
        }
        (false, Some(since)) => {
            let wait = SETTLE.saturating_sub(since.elapsed());
            if wait.is_zero() {
                if app.set_dock_visibility(false).is_ok() {
                    *shown = None;
                }
            } else {
                let app = app.clone();
                let label = label.to_owned();
                std::thread::spawn(move || {
                    std::thread::sleep(wait);
                    update(&app, &label, false);
                });
            }
        }
        _ => {}
    }
}

#[cfg(not(target_os = "macos"))]
pub fn update(_app: &AppHandle, _label: &str, _closing: bool) {}

#[cfg(target_os = "macos")]
fn refocus(app: &AppHandle, label: &str) {
    let app = app.clone();
    let label = label.to_owned();
    std::thread::spawn(move || {
        std::thread::sleep(REFOCUS);
        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.set_focus();
        }
    });
}
