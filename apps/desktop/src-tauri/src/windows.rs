//! The overlay and settings windows.

use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Monitor, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};

use crate::settings::{Corner, Settings};
use crate::{i18n, tray, AppState};

pub const OVERLAY: &str = "overlay";
pub const SETTINGS: &str = "settings";

/// Distance from the screen edge, in logical pixels.
const MARGIN: f64 = 16.0;

pub fn create_overlay(app: &AppHandle, settings: &Settings) -> tauri::Result<WebviewWindow> {
    let (width, height) = settings.overlay_size();
    let position = settings
        .overlay_position
        .filter(|[x, y]| fits_on_a_monitor(app, *x, *y, width, height))
        .unwrap_or_else(|| {
            corner_position(
                app.primary_monitor().ok().flatten().as_ref(),
                Corner::TopRight,
                width,
                height,
            )
        });

    let window =
        WebviewWindowBuilder::new(app, OVERLAY, WebviewUrl::App("index.html#/overlay".into()))
            .title("Singline")
            .inner_size(width, height)
            .position(position[0], position[1])
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .focused(false)
            .focusable(false)
            .accept_first_mouse(true)
            .visible(settings.overlay_visible)
            .build()?;
    window.set_ignore_cursor_events(settings.click_through)?;

    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Moved(physical) = event {
            let Some(overlay) = handle.get_webview_window(OVERLAY) else {
                return;
            };
            let scale = overlay.scale_factor().unwrap_or(1.0);
            let logical = physical.to_logical::<f64>(scale);
            let state = handle.state::<AppState>();
            state.update_settings_quietly(|settings| {
                settings.overlay_position = Some([logical.x, logical.y])
            });
        }
    });
    Ok(window)
}

/// Resizes the overlay to the layout and keeps it on its screen.
pub fn apply_size(app: &AppHandle, settings: &Settings) {
    let Some(window) = app.get_webview_window(OVERLAY) else {
        return;
    };
    let (width, height) = settings.overlay_size();
    let _ = window.set_size(LogicalSize::new(width, height));
    if let (Ok(Some(monitor)), Ok(position), Ok(scale)) = (
        window.current_monitor(),
        window.outer_position(),
        window.scale_factor(),
    ) {
        let current = position.to_logical::<f64>(scale);
        let [x, y] = clamp_into(&monitor, current.x, current.y, width, height);
        if (x - current.x).abs() > 0.5 || (y - current.y).abs() > 0.5 {
            let _ = window.set_position(LogicalPosition::new(x, y));
        }
    }
}

pub fn set_click_through(app: &AppHandle, enabled: bool) {
    if let Some(window) = app.get_webview_window(OVERLAY) {
        let _ = window.set_ignore_cursor_events(enabled);
    }
}

pub fn set_overlay_visible(app: &AppHandle, visible: bool) {
    if let Some(window) = app.get_webview_window(OVERLAY) {
        let _ = if visible {
            window.show()
        } else {
            window.hide()
        };
        if visible {
            // Some window managers forget "always on top" while a window is hidden.
            let _ = window.set_always_on_top(true);
        }
    }
}

pub fn toggle_overlay(app: &AppHandle) {
    let state = app.state::<AppState>();
    let visible = !state.settings().overlay_visible;
    state.update_settings(app, |settings| settings.overlay_visible = visible);
}

pub fn move_to_corner(app: &AppHandle, corner: Corner) {
    let Some(window) = app.get_webview_window(OVERLAY) else {
        return;
    };
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let settings = app.state::<AppState>().settings();
    let (width, height) = settings.overlay_size();
    let [x, y] = corner_position(monitor.as_ref(), corner, width, height);
    let _ = window.set_position(LogicalPosition::new(x, y));
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let title = i18n::Texts::new(i18n::resolve(app.state::<AppState>().settings().language))
        .get("settings.title");
    let built = WebviewWindowBuilder::new(
        app,
        SETTINGS,
        WebviewUrl::App("index.html#/settings".into()),
    )
    .title(title)
    .inner_size(560.0, 620.0)
    .min_inner_size(480.0, 420.0)
    .resizable(true)
    .center()
    .focused(true)
    .build();
    if let Err(error) = built {
        eprintln!("could not open the settings: {error}");
    }
}

pub fn set_settings_title(app: &AppHandle, title: &str) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.set_title(title);
    }
}

/// Opening the app again makes the overlay reachable: visible, clickable, with settings open.
pub fn reopen(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.update_settings(app, |settings| {
        settings.click_through = false;
        settings.overlay_visible = true;
    });
    open_settings(app);
    tray::refresh(app);
}

fn work_area(monitor: &Monitor) -> (f64, f64, f64, f64) {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let position = area.position.to_logical::<f64>(scale);
    let size = area.size.to_logical::<f64>(scale);
    (position.x, position.y, size.width, size.height)
}

fn corner_position(monitor: Option<&Monitor>, corner: Corner, width: f64, height: f64) -> [f64; 2] {
    let Some(monitor) = monitor else {
        return [MARGIN, MARGIN];
    };
    let (x, y, area_width, area_height) = work_area(monitor);
    let left = x + MARGIN;
    let right = x + area_width - MARGIN - width;
    let top = y + MARGIN;
    let bottom = y + area_height - MARGIN - height;
    match corner {
        Corner::TopLeft => [left, top],
        Corner::TopRight => [right.max(left), top],
        Corner::BottomLeft => [left, bottom.max(top)],
        Corner::BottomRight => [right.max(left), bottom.max(top)],
    }
}

fn clamp_into(monitor: &Monitor, x: f64, y: f64, width: f64, height: f64) -> [f64; 2] {
    let (left, top, area_width, area_height) = work_area(monitor);
    let max_x = (left + area_width - width).max(left);
    let max_y = (top + area_height - height).max(top);
    [x.clamp(left, max_x), y.clamp(top, max_y)]
}

fn fits_on_a_monitor(app: &AppHandle, x: f64, y: f64, width: f64, height: f64) -> bool {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|monitor| {
            let (left, top, area_width, area_height) = work_area(monitor);
            // At least a strip of the overlay must be on the screen to grab it.
            x + width > left + 40.0
                && x < left + area_width - 40.0
                && y >= top - 1.0
                && y < top + area_height - 30.0
        })
        && height > 0.0
}
