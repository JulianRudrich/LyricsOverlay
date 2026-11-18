//! The connection to the Singline browser extension: it runs next to the
//! system's players, and the settings pair browsers through these commands.

use std::path::PathBuf;

use media_session::bridge::{Bridge, BridgeBackend, BridgeOptions, BrowserStatus};
use serde::Serialize;
use tauri::State;

/// `None` in the demo, which only plays its built-in song.
pub struct BrowserBridge(Option<Bridge>);

impl BrowserBridge {
    /// Starts listening for the extension. `SINGLINE_BRIDGE_PORT` picks the
    /// port, for tests; with `debug`, the first pairing code is printed.
    pub fn start(store: Option<PathBuf>, version: &str, demo: bool, debug: bool) -> Self {
        if demo {
            return Self(None);
        }
        let mut options = BridgeOptions::new(version);
        options.store = store;
        if let Some(port) = std::env::var("SINGLINE_BRIDGE_PORT")
            .ok()
            .and_then(|port| port.parse().ok())
        {
            options.ports = vec![port];
        }
        let bridge = Bridge::start(options);
        if debug {
            if let Some(code) = bridge.pairing_code() {
                eprintln!("bridge-pairing-code {code}");
            }
        }
        Self(Some(bridge))
    }

    pub fn backend(&self) -> Option<BridgeBackend> {
        self.0.as_ref().map(Bridge::backend)
    }

    fn view(&self, with_code: bool) -> BridgeView {
        let Some(bridge) = &self.0 else {
            return BridgeView::default();
        };
        let status = bridge.status();
        BridgeView {
            available: true,
            pairing_code: if with_code && status.port.is_some() {
                bridge.pairing_code()
            } else {
                None
            },
            port: status.port,
            error: status.error,
            browsers: status.browsers,
        }
    }
}

/// What the settings show about the extension.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeView {
    /// False in the demo.
    pub available: bool,
    pub port: Option<u16>,
    pub error: Option<String>,
    /// The code to enter in the extension; valid while the settings show it.
    pub pairing_code: Option<String>,
    pub browsers: Vec<BrowserStatus>,
}

/// The bridge's state and a pairing code. The settings call this while they
/// show the code, which keeps pairing open.
#[tauri::command]
pub fn browser_bridge(bridge: State<'_, BrowserBridge>) -> BridgeView {
    bridge.view(true)
}

/// Forgets a paired browser, which then has to pair again.
#[tauri::command]
pub fn forget_browser(bridge: State<'_, BrowserBridge>, id: String) -> BridgeView {
    if let Some(running) = &bridge.0 {
        running.forget(&id);
    }
    bridge.view(true)
}
