//! The tray (menu bar) icon and its menu.

use media_session::SourcePreference;
use tauri::menu::{
    CheckMenuItemBuilder, Menu, MenuBuilder, MenuEvent, MenuItemBuilder, SubmenuBuilder,
};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

use crate::i18n::{resolve, Texts};
use crate::settings::Corner;
use crate::{windows, AppState};

const TRAY_ID: &str = "main";

const SOURCES: [(SourcePreference, &str, &str); 5] = [
    (
        SourcePreference::Automatic,
        "source:automatic",
        "source.automatic",
    ),
    (
        SourcePreference::Spotify,
        "source:spotify",
        "source.spotify",
    ),
    (
        SourcePreference::AppleMusic,
        "source:appleMusic",
        "source.appleMusic",
    ),
    (
        SourcePreference::AmazonMusic,
        "source:amazonMusic",
        "source.amazonMusic",
    ),
    (
        SourcePreference::YouTubeMusic,
        "source:youTubeMusic",
        "source.youTubeMusic",
    ),
];

const CORNERS: [(Corner, &str, &str); 4] = [
    (Corner::TopLeft, "corner:topLeft", "corner.topLeft"),
    (Corner::TopRight, "corner:topRight", "corner.topRight"),
    (Corner::BottomLeft, "corner:bottomLeft", "corner.bottomLeft"),
    (
        Corner::BottomRight,
        "corner:bottomRight",
        "corner.bottomRight",
    ),
];

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Singline")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_event);
    #[cfg(target_os = "macos")]
    {
        builder = builder
            .icon(tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray-template.png"
            ))?)
            .icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuilds the menu after the settings or the language changed.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => eprintln!("could not rebuild the tray menu: {error}"),
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let settings = app.state::<AppState>().settings();
    let texts = Texts::new(resolve(settings.language));

    let mut sources = SubmenuBuilder::new(app, texts.get("tray.source"));
    for (source, id, key) in SOURCES {
        let item = CheckMenuItemBuilder::with_id(id, texts.get(key))
            .checked(settings.source == source)
            .enabled(!app.state::<AppState>().demo)
            .build(app)?;
        sources = sources.item(&item);
    }
    let mut corners = SubmenuBuilder::new(app, texts.get("tray.position"));
    for (_, id, key) in CORNERS {
        corners = corners.item(&MenuItemBuilder::with_id(id, texts.get(key)).build(app)?);
    }

    let visibility = if settings.overlay_visible {
        "tray.hideOverlay"
    } else {
        "tray.showOverlay"
    };
    MenuBuilder::new(app)
        .item(
            &MenuItemBuilder::with_id("title", texts.get("app.name"))
                .enabled(false)
                .build(app)?,
        )
        .separator()
        .item(&MenuItemBuilder::with_id("toggle-overlay", texts.get(visibility)).build(app)?)
        .item(
            &CheckMenuItemBuilder::with_id("click-through", texts.get("tray.clickThrough"))
                .checked(settings.click_through)
                .build(app)?,
        )
        .item(&sources.build()?)
        .item(&corners.build()?)
        .item(&crate::sleep::submenu(app, &texts)?)
        .separator()
        .item(&MenuItemBuilder::with_id("stats", texts.get("tray.stats")).build(app)?)
        .item(&MenuItemBuilder::with_id("true-shuffle", texts.get("tray.trueShuffle")).build(app)?)
        .item(
            &MenuItemBuilder::with_id("settings", texts.get("tray.settings"))
                .accelerator("CmdOrCtrl+,")
                .build(app)?,
        )
        .item(
            &MenuItemBuilder::with_id("quit", texts.get("tray.quit"))
                .accelerator("CmdOrCtrl+Q")
                .build(app)?,
        )
        .build()
}

fn handle_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let state = app.state::<AppState>();
    match id {
        "toggle-overlay" => windows::toggle_overlay(app),
        "click-through" => {
            let enabled = !state.settings().click_through;
            state.update_settings(app, |settings| settings.click_through = enabled);
        }
        "settings" => windows::open_settings(app),
        "stats" => windows::open_stats(app),
        "true-shuffle" => windows::open_shuffle(app),
        "quit" => {
            state.save_settings_now();
            app.exit(0);
        }
        _ => {
            if crate::sleep::handle_menu_event(app, id) {
                return;
            }
            if let Some((source, _, _)) = SOURCES.iter().find(|(_, item, _)| *item == id) {
                let source = *source;
                state.update_settings(app, |settings| settings.source = source);
            } else if let Some((corner, _, _)) = CORNERS.iter().find(|(_, item, _)| *item == id) {
                windows::move_to_corner(app, *corner);
            }
        }
    }
}
