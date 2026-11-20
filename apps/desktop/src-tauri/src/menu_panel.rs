//! The glass panel the tray icon opens on macOS and Windows, after the macOS
//! app's menu: the song playing with the player's controls, quick switches,
//! the sleep timer and the other windows. A right-click still opens the native
//! menu, and Linux keeps only that, as its tray reports no clicks.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Theme, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::{i18n, windows, AppState};

pub const MENU: &str = "menu";

/// In logical pixels, as in the macOS app.
const WIDTH: f64 = 300.0;
/// Until the page has measured itself.
const FIRST_HEIGHT: f64 = 420.0;
const MIN_HEIGHT: f64 = 120.0;
const MAX_HEIGHT: f64 = 800.0;
/// Between the panel and the menu bar or taskbar, and the screen's edges.
/// Windows' own flyouts keep 12 px.
const GAP: f64 = if cfg!(windows) { 12.0 } else { 6.0 };
/// The glass's corners on macOS, and the page's where it paints the
/// background itself. Windows 11 rounds the window by 8 px.
const RADIUS: f64 = 16.0;
/// The click on the tray icon that took the focus away, and so closed the
/// panel, must not open it again.
const REOPEN_GUARD: Duration = Duration::from_millis(300);
/// The page fades the panel out first.
const HIDE_DELAY: Duration = Duration::from_millis(140);
/// A page that never reports its size still gets shown.
const SHOW_TIMEOUT: Duration = Duration::from_millis(1500);

/// A rectangle in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn bottom(&self) -> f64 {
        self.y + self.height
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
}

/// The top-left corner for a panel of `width` × `height` next to the tray
/// icon at `icon`: below a menu bar or taskbar at the top, above one at the
/// bottom, beside one at the left or right, `gap` away from it, centred on the
/// icon where it can be and always inside `area`, the work area of the icon's
/// screen. The side facing the icon stays put when only the height changes.
pub fn place(icon: Bounds, width: f64, height: f64, area: Bounds, gap: f64) -> (f64, f64) {
    let below = icon.y + icon.height / 2.0 <= area.y + area.height / 2.0;
    // A taskbar at the left or right leaves the icon beside the work area.
    let beside = icon.right() <= area.x || icon.x >= area.right();
    let x = if !beside {
        icon.x + icon.width / 2.0 - width / 2.0
    } else if icon.x >= area.right() {
        area.right() - gap - width
    } else {
        area.x + gap
    };
    let y = match (beside, below) {
        (false, true) => icon.bottom().max(area.y) + gap,
        (false, false) => icon.y.min(area.bottom()) - gap - height,
        (true, true) => icon.y,
        (true, false) => icon.bottom() - height,
    };
    (
        clamp_span(x, width, area.x, area.right(), gap),
        clamp_span(y, height, area.y, area.bottom(), gap),
    )
}

/// Keeps `start .. start + length` inside `min .. max`, `margin` away from
/// both ends where it fits, else from the start.
fn clamp_span(start: f64, length: f64, min: f64, max: f64, margin: f64) -> f64 {
    let low = min + margin;
    let high = max - margin - length;
    if high < low {
        (max - length).max(min).min(low)
    } else {
        start.clamp(low, high)
    }
}

/// The tray icon the panel opened from, the work area of its screen and that
/// screen's scale.
#[derive(Debug, Clone, Copy)]
struct Spot {
    icon: Bounds,
    area: Bounds,
    scale: f64,
}

#[derive(Default)]
pub struct MenuPanel {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    spot: Option<Spot>,
    /// The page's height in logical pixels, as it last measured itself.
    height: Option<f64>,
    open: bool,
    /// The window was just built and waits for the page to measure itself.
    waiting: bool,
    closed_at: Option<Instant>,
    /// Counts the openings, so a late hide never hides a newer one.
    generation: u64,
}

impl MenuPanel {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Tells the page to play its entrance or to fade out.
#[derive(Clone, Serialize)]
struct PanelEvent {
    open: bool,
}

/// A click on the tray icon at `icon`: opens the panel there, or closes it
/// when it is open.
pub fn toggle(app: &AppHandle, icon: Bounds) {
    let Some(panel) = app.try_state::<MenuPanel>() else {
        return;
    };
    let (open, just_closed) = {
        let inner = panel.lock();
        let just_closed = inner
            .closed_at
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD);
        (inner.open, just_closed)
    };
    if open {
        close(app);
    } else if !just_closed {
        open_at(app, icon);
    }
}

fn open_at(app: &AppHandle, icon: Bounds) {
    let spot = spot_for(app, icon);
    let panel = app.state::<MenuPanel>();
    let existing = app.get_webview_window(MENU);
    let (height, generation) = {
        let mut inner = panel.lock();
        inner.open = true;
        inner.generation += 1;
        inner.spot = Some(spot);
        inner.waiting = existing.is_none();
        (inner.height, inner.generation)
    };
    if let Some(window) = existing {
        show(app, &window, spot, height.unwrap_or(FIRST_HEIGHT));
        return;
    }
    if let Err(error) = create(app) {
        log::error!("could not open the menu: {error}");
        let mut inner = panel.lock();
        inner.open = false;
        inner.waiting = false;
        return;
    }
    // It shows once the page has measured itself; this covers a page that never does.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SHOW_TIMEOUT).await;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || reveal(&handle, generation));
    });
}

/// Places the window, brings it to the front with the focus, and lets the
/// page play its entrance.
fn show(app: &AppHandle, window: &WebviewWindow, spot: Spot, height: f64) {
    place_window(window, spot, height);
    let _ = app.emit_to(MENU, "menu-panel", PanelEvent { open: true });
    let _ = window.show();
    let _ = window.set_focus();
}

/// Shows a newly built panel once its page is ready, unless it closed meanwhile.
fn reveal(app: &AppHandle, generation: u64) {
    let panel = app.state::<MenuPanel>();
    let (spot, height) = {
        let mut inner = panel.lock();
        if !inner.waiting || !inner.open || inner.generation != generation {
            return;
        }
        inner.waiting = false;
        (inner.spot, inner.height.unwrap_or(FIRST_HEIGHT))
    };
    if let (Some(spot), Some(window)) = (spot, app.get_webview_window(MENU)) {
        show(app, &window, spot, height);
    }
}

/// Closes the panel: the page fades it out, then it hides. The window stays,
/// so the next opening is immediate.
pub fn close(app: &AppHandle) {
    let Some(panel) = app.try_state::<MenuPanel>() else {
        return;
    };
    let generation = {
        let mut inner = panel.lock();
        if !inner.open {
            return;
        }
        inner.open = false;
        inner.waiting = false;
        inner.closed_at = Some(Instant::now());
        inner.generation
    };
    let _ = app.emit_to(MENU, "menu-panel", PanelEvent { open: false });
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(HIDE_DELAY).await;
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let panel = handle.state::<MenuPanel>();
            let closed = {
                let inner = panel.lock();
                !inner.open && inner.generation == generation
            };
            if let Some(window) = handle.get_webview_window(MENU).filter(|_| closed) {
                let _ = window.hide();
            }
        });
    });
}

/// Builds the hidden window. The page says how it is drawn: glass from the
/// system, or its own background with the corners rounded to `radius`.
fn create(app: &AppHandle) -> tauri::Result<()> {
    let glass = windows::glass(RADIUS);
    let radius = if cfg!(windows) { 8.0 } else { RADIUS };
    let look = if glass.is_some() { "glass&" } else { "" };
    let url = format!("index.html?{look}radius={radius}#/menu");
    let title = i18n::Texts::new(i18n::resolve(app.state::<AppState>().settings().language))
        .get("app.name");
    let mut builder = WebviewWindowBuilder::new(app, MENU, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(WIDTH, FIRST_HEIGHT)
        .decorations(false)
        .transparent(true)
        // GTK keeps a fixed-size window at its first size, and this one
        // follows its page; it has no edges to drag anyway.
        .resizable(cfg!(target_os = "linux"))
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .skip_taskbar(true)
        .theme(Some(Theme::Dark))
        // With glass, macOS shades it and Windows 11 also rounds its corners.
        .shadow(glass.is_some())
        .visible(false);
    if let Some(effects) = glass {
        builder = builder.effects(effects);
    }
    let window = builder.build()?;
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            let waiting = handle.state::<MenuPanel>().lock().waiting;
            if !waiting {
                close(&handle);
            }
        }
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            close(&handle);
        }
        _ => {}
    });
    Ok(())
}

/// Sizes the window for a page `height` logical pixels tall and places it at
/// the tray icon.
fn place_window(window: &WebviewWindow, spot: Spot, height: f64) {
    let width = (WIDTH * spot.scale).round();
    let height = (height.clamp(MIN_HEIGHT, MAX_HEIGHT) * spot.scale).round();
    let (x, y) = place(
        spot.icon,
        width,
        height,
        spot.area,
        (GAP * spot.scale).round(),
    );
    let size = PhysicalSize::new(width as u32, height as u32);
    let _ = window.set_size(size);
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    // Windows rescales a window that moves onto a screen with another scale.
    if cfg!(windows) {
        let _ = window.set_size(size);
    }
}

/// The work area and scale of the screen the tray icon is on.
fn spot_for(app: &AppHandle, icon: Bounds) -> Spot {
    let (x, y) = (icon.x + icon.width / 2.0, icon.y + icon.height / 2.0);
    let monitor = app
        .available_monitors()
        .unwrap_or_default()
        .into_iter()
        .find(|monitor| {
            let (position, size) = (monitor.position(), monitor.size());
            Bounds {
                x: f64::from(position.x),
                y: f64::from(position.y),
                width: f64::from(size.width),
                height: f64::from(size.height),
            }
            .contains(x, y)
        })
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return Spot {
            icon,
            area: Bounds {
                x: 0.0,
                y: 0.0,
                width: 1280.0,
                height: 800.0,
            },
            scale: 1.0,
        };
    };
    let area = monitor.work_area();
    Spot {
        icon,
        area: Bounds {
            x: f64::from(area.position.x),
            y: f64::from(area.position.y),
            width: f64::from(area.size.width),
            height: f64::from(area.size.height),
        },
        scale: monitor.scale_factor(),
    }
}

/// The tray icon's bounds as a click on it reports them.
#[cfg(any(target_os = "macos", windows))]
pub fn icon_bounds(app: &AppHandle, rect: tauri::Rect) -> Bounds {
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    Bounds {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

/// With SINGLINE_DEBUG, `--menu-panel` opens the panel as if a tray icon at
/// the top right of the main screen was clicked, as Linux reports no clicks.
pub fn open_for_debug(app: &AppHandle) {
    let Some(monitor) = app.primary_monitor().ok().flatten() else {
        return;
    };
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let icon = Bounds {
        x: f64::from(area.position.x) + f64::from(area.size.width) - 64.0 * scale,
        y: f64::from(area.position.y),
        width: 24.0 * scale,
        height: 24.0 * scale,
    };
    toggle(app, icon);
}

/// The page measured itself: sizes the panel to it, and shows a new panel
/// that waited for this.
#[tauri::command]
pub fn menu_panel_resize(app: AppHandle, height: f64) {
    if !height.is_finite() {
        return;
    }
    if app.state::<AppState>().debug {
        log::debug!(target: crate::logging::CHECKS, "menu-panel-height {height}");
    }
    let panel = app.state::<MenuPanel>();
    let (spot, waiting, generation) = {
        let mut inner = panel.lock();
        inner.height = Some(height);
        (
            inner.spot.filter(|_| inner.open),
            inner.waiting,
            inner.generation,
        )
    };
    // A closed panel takes the new height when it opens again.
    let Some(spot) = spot else {
        return;
    };
    if waiting {
        reveal(&app, generation);
    } else if let Some(window) = app.get_webview_window(MENU) {
        place_window(&window, spot, height);
    }
}

#[tauri::command]
pub fn close_menu_panel(app: AppHandle) {
    close(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(x: f64, y: f64, width: f64, height: f64) -> Bounds {
        Bounds {
            x,
            y,
            width,
            height,
        }
    }

    /// A Retina screen whose menu bar is 74 px tall.
    const MAC: Bounds = Bounds {
        x: 0.0,
        y: 74.0,
        width: 2880.0,
        height: 1726.0,
    };

    #[test]
    fn opens_below_the_menu_bar_centred_on_the_icon() {
        let icon = bounds(2400.0, 0.0, 44.0, 74.0);
        assert_eq!(place(icon, 600.0, 800.0, MAC, 12.0), (2122.0, 86.0));
    }

    #[test]
    fn stays_on_the_screen() {
        let icon = bounds(2840.0, 0.0, 40.0, 74.0);
        assert_eq!(place(icon, 600.0, 800.0, MAC, 12.0), (2268.0, 86.0));
        let icon = bounds(2400.0, 0.0, 44.0, 74.0);
        assert_eq!(
            place(icon, 600.0, 2000.0, MAC, 12.0).1,
            74.0,
            "a panel taller than the screen starts at its top"
        );
    }

    #[test]
    fn opens_next_to_a_taskbar_on_any_side() {
        let bottom = bounds(0.0, 0.0, 1920.0, 1032.0);
        let icon = bounds(1700.0, 1040.0, 24.0, 24.0);
        assert_eq!(place(icon, 300.0, 400.0, bottom, 12.0), (1562.0, 620.0));

        let top = bounds(0.0, 48.0, 1920.0, 1032.0);
        let icon = bounds(1700.0, 12.0, 24.0, 24.0);
        assert_eq!(place(icon, 300.0, 400.0, top, 12.0), (1562.0, 60.0));

        let right = bounds(0.0, 0.0, 1856.0, 1080.0);
        let icon = bounds(1880.0, 1000.0, 24.0, 24.0);
        assert_eq!(place(icon, 300.0, 400.0, right, 12.0), (1544.0, 624.0));

        let left = bounds(64.0, 0.0, 1856.0, 1080.0);
        let icon = bounds(20.0, 100.0, 24.0, 24.0);
        assert_eq!(place(icon, 300.0, 400.0, left, 12.0), (76.0, 100.0));
    }

    #[test]
    fn works_on_a_screen_left_of_the_main_one() {
        let area = bounds(-1920.0, 0.0, 1920.0, 1040.0);
        let icon = bounds(-200.0, 1050.0, 24.0, 24.0);
        assert_eq!(place(icon, 300.0, 400.0, area, 12.0), (-338.0, 628.0));
    }

    #[test]
    fn keeps_the_side_facing_the_icon_when_the_height_changes() {
        let area = bounds(0.0, 0.0, 1920.0, 1032.0);
        let icon = bounds(1700.0, 1040.0, 24.0, 24.0);
        let (_, short) = place(icon, 300.0, 400.0, area, 12.0);
        let (_, tall) = place(icon, 300.0, 520.0, area, 12.0);
        assert_eq!(
            short + 400.0,
            tall + 520.0,
            "the bottom stays above the taskbar"
        );

        let icon = bounds(2400.0, 0.0, 44.0, 74.0);
        let (_, short) = place(icon, 600.0, 800.0, MAC, 12.0);
        let (_, tall) = place(icon, 600.0, 1040.0, MAC, 12.0);
        assert_eq!(short, tall, "the top stays under the menu bar");
    }
}
