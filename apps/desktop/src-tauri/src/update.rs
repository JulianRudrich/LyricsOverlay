//! Updates: Singline asks GitHub for a newer version at start and every few
//! hours, downloads it in the background and offers to restart into it. Test
//! builds, whose version carries a build number such as 0.4.0-67, follow the
//! test channel; releases follow the releases. Every download is checked
//! against the signature of the key the app was built with.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::AppState;

pub const EVENT: &str = "update";
/// The first check waits until Singline has settled.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
const RELEASES: &str =
    "https://github.com/JulianRudrich/Singline/releases/latest/download/latest.json";
const TEST_BUILDS: &str =
    "https://github.com/JulianRudrich/Singline/releases/download/nightly/latest.json";

/// What the settings and the tray show about updates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateView {
    /// Not checked yet, or the listener turned automatic checks off.
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
        progress: Option<f64>,
    },
    /// Downloaded and checked; a restart installs it.
    Ready {
        version: String,
    },
    Failed {
        error: String,
    },
    /// This build cannot update itself: a development build or a test run.
    Unavailable,
}

pub struct Updates {
    view: Mutex<UpdateView>,
    ready: Mutex<Option<(Update, Vec<u8>)>>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            view: Mutex::new(UpdateView::Idle),
            ready: Mutex::new(None),
        }
    }
}

impl Updates {
    pub fn view(&self) -> UpdateView {
        self.view.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Where this build looks for updates: test builds carry a pre-release part.
pub fn channel(version: &str) -> &'static str {
    if version.contains('-') {
        TEST_BUILDS
    } else {
        RELEASES
    }
}

/// Development builds, the demo and test runs never update, and on Linux only
/// the AppImage can replace itself; a .deb updates through the package manager.
fn available(app: &AppHandle) -> bool {
    let demo = app.try_state::<AppState>().is_none_or(|state| state.demo);
    let replaceable = !cfg!(target_os = "linux") || std::env::var_os("APPIMAGE").is_some();
    !cfg!(debug_assertions)
        && !demo
        && replaceable
        && std::env::var_os("SINGLINE_DATA_DIR").is_none()
}

fn set(app: &AppHandle, view: UpdateView) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let changed = {
        let mut current = updates.view.lock().unwrap_or_else(|p| p.into_inner());
        let changed = *current != view;
        *current = view.clone();
        changed
    };
    if changed {
        let _ = app.emit(EVENT, &view);
        if matches!(view, UpdateView::Ready { .. } | UpdateView::UpToDate) {
            crate::tray::refresh(app);
        }
    }
}

/// Checks at start and then every few hours, while automatic checks are on.
pub fn start(app: &AppHandle) {
    if !available(app) {
        set(app, UpdateView::Unavailable);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            let automatic = app.state::<AppState>().settings().auto_update;
            if automatic {
                check(&app).await;
            }
            tokio::time::sleep(INTERVAL).await;
        }
    });
}

/// Looks for a newer version and downloads it. A downloaded one is kept for
/// the restart; checking again does not fetch it twice.
pub async fn check(app: &AppHandle) {
    if !available(app) {
        set(app, UpdateView::Unavailable);
        return;
    }
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    if matches!(
        updates.view(),
        UpdateView::Checking | UpdateView::Downloading { .. } | UpdateView::Ready { .. }
    ) {
        return;
    }
    set(app, UpdateView::Checking);
    let version = app.package_info().version.to_string();
    let found = async {
        let endpoint = channel(&version)
            .parse()
            .map_err(|_| "bad update address".to_owned())?;
        let updater = app
            .updater_builder()
            .endpoints(vec![endpoint])
            .and_then(|builder| builder.build())
            .map_err(|error| error.to_string())?;
        updater.check().await.map_err(|error| error.to_string())
    }
    .await;
    let update = match found {
        Ok(Some(update)) => update,
        Ok(None) => return set(app, UpdateView::UpToDate),
        Err(error) => {
            log::warn!("could not check for updates: {error}");
            return set(app, UpdateView::Failed { error });
        }
    };
    let next = update.version.clone();
    log::info!("downloading Singline {next}");
    set(
        app,
        UpdateView::Downloading {
            version: next.clone(),
            progress: Some(0.0),
        },
    );
    let mut received = 0usize;
    let progress_app = app.clone();
    let progress_version = next.clone();
    let downloaded = update
        .download(
            move |chunk, total| {
                received += chunk;
                let progress = total
                    .filter(|total| *total > 0)
                    .map(|total| received as f64 / total as f64);
                set(
                    &progress_app,
                    UpdateView::Downloading {
                        version: progress_version.clone(),
                        // Whole percents, so the settings redraw a hundred times at most.
                        progress: progress.map(|value| (value * 100.0).floor() / 100.0),
                    },
                );
            },
            || {},
        )
        .await;
    match downloaded {
        Ok(bytes) => {
            *updates.ready.lock().unwrap_or_else(|p| p.into_inner()) = Some((update, bytes));
            set(app, UpdateView::Ready { version: next });
        }
        Err(error) => {
            log::warn!("could not download Singline {next}: {error}");
            set(
                app,
                UpdateView::Failed {
                    error: error.to_string(),
                },
            );
        }
    }
}

/// Installs the downloaded version and starts it. On Windows the installer
/// takes over and starts Singline again itself.
pub fn restart_into_update(app: &AppHandle) {
    let Some(updates) = app.try_state::<Updates>() else {
        return;
    };
    let Some((update, bytes)) = updates
        .ready
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    else {
        return;
    };
    // Windows ends the app while it installs, so everything is saved first.
    crate::now::clear(app);
    crate::history::flush(app);
    app.state::<AppState>().save_settings_now();
    match update.install(bytes) {
        Ok(()) => app.restart(),
        Err(error) => {
            log::error!("could not install Singline {}: {error}", update.version);
            set(
                app,
                UpdateView::Failed {
                    error: error.to_string(),
                },
            );
        }
    }
}

#[tauri::command]
pub fn update_view(updates: tauri::State<'_, Updates>) -> UpdateView {
    updates.view()
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateView {
    check(&app).await;
    app.state::<Updates>().view()
}

#[tauri::command]
pub fn update_install(app: AppHandle) {
    restart_into_update(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builds_follow_the_test_channel() {
        assert_eq!(channel("0.4.0-67"), TEST_BUILDS);
        assert_eq!(channel("0.4.0"), RELEASES);
        assert_eq!(channel("1.0.0"), RELEASES);
    }

    #[test]
    fn the_view_reads_well_in_the_interface() {
        let view = UpdateView::Downloading {
            version: "0.4.0-68".into(),
            progress: Some(0.5),
        };
        assert_eq!(
            serde_json::to_string(&view).expect("serialises"),
            r#"{"kind":"downloading","version":"0.4.0-68","progress":0.5}"#
        );
        assert_eq!(
            serde_json::to_string(&UpdateView::UpToDate).expect("serialises"),
            r#"{"kind":"upToDate"}"#
        );
    }
}
