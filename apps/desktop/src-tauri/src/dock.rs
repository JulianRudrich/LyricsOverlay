//! On macOS Singline lives in the menu bar. While one of its windows is open
//! it also shows in the Dock, so it can be found in the app switcher and in
//! Force Quit, and the menu bar offers its app menu with Quit. Without a
//! window it leaves the Dock again, which keeps the overlay above apps in full
//! screen.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

#[cfg(target_os = "macos")]
use std::sync::Mutex;
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::time::Instant;

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

/// What happens to the Dock icon.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Show,
    Hide,
    /// Hide once the icon has shown for this much longer.
    HideAfter(Duration),
    Keep,
}

/// `open` tells whether one of the windows is open, `shown_for` how long the
/// icon has been in the Dock, if it is.
fn step(open: bool, shown_for: Option<Duration>) -> Step {
    match (open, shown_for) {
        (true, None) => Step::Show,
        (false, Some(elapsed)) if elapsed >= SETTLE => Step::Hide,
        (false, Some(elapsed)) => Step::HideAfter(SETTLE - elapsed),
        _ => Step::Keep,
    }
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
    match step(open, shown.map(|since| since.elapsed())) {
        Step::Show => {
            if app.set_dock_visibility(true).is_ok() {
                *shown = Some(Instant::now());
                refocus(app, label);
            }
        }
        Step::Hide => {
            if app.set_dock_visibility(false).is_ok() {
                *shown = None;
            }
        }
        Step::HideAfter(wait) => {
            let app = app.clone();
            let label = label.to_owned();
            std::thread::spawn(move || {
                std::thread::sleep(wait);
                update(&app, &label, false);
            });
        }
        Step::Keep => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_window_brings_the_icon() {
        assert_eq!(step(true, None), Step::Show);
    }

    #[test]
    fn the_icon_stays_while_a_window_is_open() {
        assert_eq!(step(true, Some(Duration::from_secs(30))), Step::Keep);
    }

    #[test]
    fn the_icon_leaves_with_the_last_window() {
        assert_eq!(step(false, Some(Duration::from_secs(30))), Step::Hide);
        assert_eq!(step(false, Some(SETTLE)), Step::Hide);
    }

    #[test]
    fn a_window_closed_at_once_waits_until_the_dock_lets_go() {
        assert_eq!(
            step(false, Some(Duration::from_millis(200))),
            Step::HideAfter(Duration::from_millis(1000))
        );
    }

    #[test]
    fn nothing_changes_without_icon_or_window() {
        assert_eq!(step(false, None), Step::Keep);
    }
}
