//! The sleep timer in the tray menu: pauses the followed player after a set
//! time or just before the song ends. It lives only as long as the app runs.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use lyrics_core::sleep::{minutes_left, SleepMode, SleepTimer};
use media_session::Command;
use tauri::menu::{CheckMenuItem, CheckMenuItemBuilder, Submenu, SubmenuBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::i18n::{resolve, Texts};
use crate::playback::{PlaybackView, Request};
use crate::{tray, AppState};

const PREFIX: &str = "sleep:";

/// Sending the pause takes a moment too, most of all through AppleScript.
const COMMAND_DELAY: f64 = 0.25;

#[derive(Default)]
pub struct SleepState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    timer: Option<SleepTimer>,
    /// The followed song at the last reading.
    track_key: Option<String>,
    /// Seconds until the pause at the last reading.
    remaining: Option<f64>,
    /// The menu built last, to update it without rebuilding.
    menu: Option<SleepMenu>,
}

struct SleepMenu {
    submenu: Submenu<Wry>,
    end_of_track: CheckMenuItem<Wry>,
    /// What the menu shows now, so it is only touched on a change.
    label: Label,
    end_enabled: bool,
}

/// The title of the submenu, which shows the time left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Label {
    Off,
    MinutesLeft(u64),
    AtEndOfTrack,
}

impl Label {
    fn of(timer: Option<&SleepTimer>, remaining: Option<f64>) -> Self {
        match (timer, remaining) {
            (None, _) => Label::Off,
            (Some(_), Some(seconds)) => Label::MinutesLeft(minutes_left(seconds).max(1)),
            (Some(_), None) => Label::AtEndOfTrack,
        }
    }

    fn text(self, texts: &Texts) -> String {
        match self {
            Label::Off => texts.get("tray.sleep"),
            Label::MinutesLeft(minutes) => texts
                .get("tray.sleepLeft")
                .replace("{minutes}", &minutes.to_string()),
            Label::AtEndOfTrack => texts.get("tray.sleepAtEnd"),
        }
    }
}

fn item_id(mode: Option<SleepMode>) -> String {
    match mode {
        None => format!("{PREFIX}off"),
        Some(SleepMode::Minutes(minutes)) => format!("{PREFIX}{minutes}"),
        Some(SleepMode::EndOfTrack) => format!("{PREFIX}end"),
    }
}

/// The choice behind a menu item: `Some(None)` turns the timer off, `None`
/// means the item is not part of the sleep timer.
fn choice(id: &str) -> Option<Option<SleepMode>> {
    match id.strip_prefix(PREFIX)? {
        "off" => Some(None),
        _ => SleepMode::CHOICES
            .into_iter()
            .find(|mode| item_id(Some(*mode)) == id)
            .map(Some),
    }
}

/// The submenu for the tray, showing the running timer.
pub fn submenu(app: &AppHandle, texts: &Texts) -> tauri::Result<Submenu<Wry>> {
    let Some(state) = app.try_state::<SleepState>() else {
        return SubmenuBuilder::new(app, texts.get("tray.sleep")).build();
    };
    let (mode, label, has_track) = {
        let inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
        let timer = inner.timer.as_ref();
        (
            timer.map(SleepTimer::mode),
            Label::of(timer, inner.remaining),
            inner.track_key.is_some(),
        )
    };
    // Menu items are built on the main thread, so no lock is held meanwhile.
    let off = CheckMenuItemBuilder::with_id(item_id(None), texts.get("sleep.off"))
        .checked(mode.is_none())
        .build(app)?;
    let mut builder = SubmenuBuilder::new(app, label.text(texts))
        .item(&off)
        .separator();
    let end_enabled = has_track || mode == Some(SleepMode::EndOfTrack);
    let mut end_of_track = None;
    for choice in SleepMode::CHOICES {
        let text = match choice {
            SleepMode::Minutes(minutes) => texts
                .get("sleep.minutes")
                .replace("{minutes}", &minutes.to_string()),
            SleepMode::EndOfTrack => texts.get("sleep.endOfTrack"),
        };
        let item = CheckMenuItemBuilder::with_id(item_id(Some(choice)), text)
            .checked(mode == Some(choice))
            .enabled(choice != SleepMode::EndOfTrack || end_enabled)
            .build(app)?;
        builder = builder.item(&item);
        if choice == SleepMode::EndOfTrack {
            end_of_track = Some(item);
        }
    }
    let submenu = builder.build()?;
    if let Some(end_of_track) = end_of_track {
        state.inner.lock().unwrap_or_else(|p| p.into_inner()).menu = Some(SleepMenu {
            submenu: submenu.clone(),
            end_of_track,
            label,
            end_enabled,
        });
    }
    Ok(submenu)
}

/// Handles a click in the submenu. Returns false for items of other menus.
pub fn handle_menu_event(app: &AppHandle, id: &str) -> bool {
    let Some(choice) = choice(id) else {
        return false;
    };
    if let Some(state) = app.try_state::<SleepState>() {
        let mut inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        inner.timer =
            choice.and_then(|mode| SleepTimer::start(mode, now, inner.track_key.as_deref()));
        inner.remaining = match choice {
            Some(SleepMode::Minutes(minutes)) => Some(f64::from(minutes) * 60.0),
            _ => None,
        };
    }
    // Rebuilt rather than updated, so the checkmarks always match the timer.
    tray::refresh(app);
    true
}

/// After every reading of the players: pauses the followed player once the
/// timer is due and keeps the time left in the menu current. `interval` is the
/// time between two readings.
pub fn tick(app: &AppHandle, view: &PlaybackView, interval: Duration) {
    let Some(state) = app.try_state::<SleepState>() else {
        return;
    };
    let track_key = view.track.as_ref().map(|track| track.key.clone());
    let track_left = view.track.as_ref().and_then(|track| {
        let duration = track.duration?;
        Some((duration - view.position?).max(0.0))
    });
    let lead = interval.as_secs_f64() + COMMAND_DELAY;

    let mut inner = state.inner.lock().unwrap_or_else(|p| p.into_inner());
    inner.track_key = track_key;
    let Inner {
        timer,
        track_key,
        remaining,
        menu,
    } = &mut *inner;
    *remaining = timer
        .as_mut()
        .and_then(|timer| timer.remaining(Instant::now(), track_key.as_deref(), track_left, lead));
    if timer.is_some() && *remaining == Some(0.0) {
        *timer = None;
        *remaining = None;
        drop(inner);
        app.state::<AppState>()
            .request(Request::Command(Command::Pause));
        tray::refresh(app);
        return;
    }

    let label = Label::of(timer.as_ref(), *remaining);
    let end_enabled =
        track_key.is_some() || timer.as_ref().map(SleepTimer::mode) == Some(SleepMode::EndOfTrack);
    let Some(menu) = menu.as_mut() else {
        return;
    };
    let new_label = (menu.label != label).then_some(label);
    let new_enabled = (menu.end_enabled != end_enabled).then_some(end_enabled);
    menu.label = label;
    menu.end_enabled = end_enabled;
    let (submenu, end_of_track) = (menu.submenu.clone(), menu.end_of_track.clone());
    // Menu changes wait for the main thread, so the lock is released first.
    drop(inner);
    if let Some(label) = new_label {
        let texts = Texts::new(resolve(app.state::<AppState>().settings().language));
        let _ = submenu.set_text(label.text(&texts));
    }
    if let Some(enabled) = new_enabled {
        let _ = end_of_track.set_enabled(enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_items_map_to_choices() {
        assert_eq!(choice("sleep:off"), Some(None));
        assert_eq!(choice("sleep:45"), Some(Some(SleepMode::Minutes(45))));
        assert_eq!(choice("sleep:end"), Some(Some(SleepMode::EndOfTrack)));
        assert_eq!(choice("sleep:7"), None, "only the offered times");
        assert_eq!(choice("settings"), None);
        for mode in SleepMode::CHOICES {
            assert_eq!(choice(&item_id(Some(mode))), Some(Some(mode)));
        }
    }

    #[test]
    fn the_title_shows_whole_minutes_left() {
        let start = Instant::now();
        let timer = SleepTimer::start(SleepMode::Minutes(15), start, None);
        assert_eq!(Label::of(None, None), Label::Off);
        assert_eq!(
            Label::of(timer.as_ref(), Some(14.0 * 60.0 + 1.0)),
            Label::MinutesLeft(15)
        );
        assert_eq!(Label::of(timer.as_ref(), Some(0.2)), Label::MinutesLeft(1));
        assert_eq!(Label::of(timer.as_ref(), None), Label::AtEndOfTrack);
        let texts = Texts::new(crate::i18n::Lang::De);
        assert_eq!(
            Label::MinutesLeft(12).text(&texts),
            "Sleep-Timer – noch 12 Min."
        );
    }
}
