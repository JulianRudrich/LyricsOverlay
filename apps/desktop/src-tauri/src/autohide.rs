//! Auto-hide: the overlay steps aside while nothing plays. It only hides on
//! top of the listener's own choice, which stays in `settings.overlay_visible`.

use std::sync::Mutex;
use std::time::Instant;

use lyrics_core::visibility::AutoHide;
use tauri::{AppHandle, Manager};

use crate::playback::PlaybackView;
use crate::settings::Settings;
use crate::{windows, AppState};

pub struct AutoHideState {
    inner: Mutex<Inner>,
}

struct Inner {
    policy: AutoHide,
    /// What the overlay window was last set to, so it is only touched on a change.
    shown: bool,
}

impl AutoHideState {
    /// `shown` is how the overlay window starts.
    pub fn new(shown: bool) -> Self {
        Self {
            inner: Mutex::new(Inner {
                policy: AutoHide::new(Instant::now()),
                shown,
            }),
        }
    }
}

/// After every reading of the players. The open settings window keeps the
/// overlay in view, so changes to its look can be seen while nothing plays.
pub fn update(app: &AppHandle, view: &PlaybackView) {
    // Listening for a song to find counts as playing.
    let singing = app
        .try_state::<crate::sing::Sing>()
        .is_some_and(|sing| sing.is_active());
    let playing = (view.state == "track" && view.playing) || singing;
    let settings_open = app.get_webview_window(windows::SETTINGS).is_some();
    apply(app, |policy, now| {
        policy.observe(playing || settings_open, now)
    });
}

/// After the settings changed. Showing the overlay yourself starts the grace
/// period over, so it appears even while nothing plays.
pub fn settings_changed(app: &AppHandle, old: &Settings, new: &Settings) {
    let shown_again = new.overlay_visible && !old.overlay_visible;
    apply(app, |policy, now| {
        if shown_again {
            policy.wake(now);
        }
    });
}

/// Brings a hidden overlay back for a while, as when Sing to find starts.
pub fn wake(app: &AppHandle) {
    apply(app, |policy, now| policy.wake(now));
}

fn apply(app: &AppHandle, change: impl FnOnce(&mut AutoHide, Instant)) {
    let Some(state) = app.try_state::<AutoHideState>() else {
        return;
    };
    let mut inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
    // Read under the lock, so a caller with older settings cannot undo a newer change.
    let settings = app.state::<AppState>().settings();
    let now = Instant::now();
    change(&mut inner.policy, now);
    let show = inner
        .policy
        .shows(settings.overlay_visible, settings.auto_hide, now);
    if inner.shown != show {
        inner.shown = show;
        // Held under the lock, so two callers cannot leave the window and
        // `shown` disagreeing. Showing and hiding only post a message to the
        // main thread and never wait for it.
        windows::set_overlay_visible(app, show);
    }
}
