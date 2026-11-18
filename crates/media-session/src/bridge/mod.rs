//! The browser bridge: a small HTTP server on 127.0.0.1 that the Singline
//! browser extension reports to. It covers web players the system does not
//! show, or not completely, such as YouTube Music in a background tab, and on
//! macOS every web player.
//!
//! The extension pairs once with a six-digit code that the settings show and
//! then sends each request with the token it got. Everything else is refused:
//! the server never listens on other interfaces, web pages fail the origin
//! and host checks, and requests are small.

mod http;
mod pairing;
mod protocol;
mod server;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::backend::{Backend, CommandError};
use crate::model::{Command, PlayerId, PlayerSnapshot, Reading};

/// The app listens on the first free port of these; the extension tries them
/// in the same order.
pub const DEFAULT_PORTS: [u16; 3] = [17464, 17465, 17466];
/// Player ids of browser tabs start with this.
pub const PLAYER_PREFIX: &str = "bridge:";

/// A playing tab that stops reporting is gone after this long.
const PLAYING_TIMEOUT: Duration = Duration::from_secs(10);
/// Paused tabs in the background report rarely, because browsers slow them down.
const PAUSED_TIMEOUT: Duration = Duration::from_secs(120);
/// Commands not picked up by then are dropped rather than carried out late.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_COMMANDS: usize = 8;
/// A browser counts as connected while it reported this recently.
const CONNECTED: Duration = Duration::from_secs(15);

pub struct BridgeOptions {
    pub ports: Vec<u16>,
    /// Where the paired browsers are kept; `None` keeps them in memory.
    pub store: Option<PathBuf>,
    /// The app's version, for the extension.
    pub version: String,
}

impl BridgeOptions {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            ports: DEFAULT_PORTS.to_vec(),
            store: None,
            version: version.into(),
        }
    }
}

type TabKey = (String, u64);

struct Tab {
    browser: String,
    update: protocol::TabUpdate,
    sampled_at: Instant,
    received_at: Instant,
}

struct Shared {
    port: Option<u16>,
    error: Option<String>,
    version: String,
    pairing: pairing::Pairing,
    clients: pairing::Clients,
    tabs: HashMap<TabKey, Tab>,
    commands: HashMap<TabKey, Vec<(Command, Instant)>>,
}

impl Shared {
    fn new(options: &BridgeOptions) -> Self {
        Self {
            port: None,
            error: None,
            version: options.version.clone(),
            pairing: pairing::Pairing::default(),
            clients: pairing::Clients::load(options.store.clone()),
            tabs: HashMap::new(),
            commands: HashMap::new(),
        }
    }

    fn expire(&mut self, now: Instant) {
        self.tabs.retain(|_, tab| {
            let timeout = if tab.update.playing() {
                PLAYING_TIMEOUT
            } else {
                PAUSED_TIMEOUT
            };
            now.saturating_duration_since(tab.received_at) <= timeout
        });
        let tabs = &self.tabs;
        self.commands.retain(|key, queue| {
            queue.retain(|(_, at)| now.saturating_duration_since(*at) <= COMMAND_TIMEOUT);
            tabs.contains_key(key) && !queue.is_empty()
        });
    }

    fn forget(&mut self, client: &str) {
        self.tabs.retain(|(owner, _), _| owner != client);
        self.commands.retain(|(owner, _), _| owner != client);
    }
}

fn player_id(client: &str, tab: u64) -> PlayerId {
    PlayerId::new(format!("{PLAYER_PREFIX}{client}:{tab}"))
}

fn tab_key(player: &PlayerId) -> Option<TabKey> {
    let (client, tab) = player
        .as_str()
        .strip_prefix(PLAYER_PREFIX)?
        .rsplit_once(':')?;
    Some((client.to_owned(), tab.parse().ok()?))
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1_000.0)
}

/// What the settings show about the bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    /// The port the bridge listens on, once it does.
    pub port: Option<u16>,
    /// Why it does not listen, for example because every port is taken.
    pub error: Option<String>,
    pub browsers: Vec<BrowserStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub id: String,
    pub name: String,
    /// Seconds since 1970.
    pub paired_at: u64,
    /// The browser reported in the last few seconds.
    pub connected: bool,
    /// Tabs it currently reports.
    pub tabs: usize,
}

/// The running bridge. Cloning it gives another handle to the same server.
#[derive(Clone)]
pub struct Bridge {
    shared: Arc<Mutex<Shared>>,
}

impl Bridge {
    /// Starts the server in the background. When every port is taken, it
    /// says so in [`Bridge::status`] and tries again a little later.
    pub fn start(options: BridgeOptions) -> Self {
        let shared = Arc::new(Mutex::new(Shared::new(&options)));
        server::spawn(Arc::clone(&shared), options.ports);
        Self { shared }
    }

    fn shared(&self) -> MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The backend that turns the reported tabs into readings.
    pub fn backend(&self) -> BridgeBackend {
        BridgeBackend {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn status(&self) -> BridgeStatus {
        let now = Instant::now();
        let mut shared = self.shared();
        shared.expire(now);
        let browsers = shared
            .clients
            .list()
            .into_iter()
            .map(|(client, paired_at, last_seen)| BrowserStatus {
                tabs: shared
                    .tabs
                    .keys()
                    .filter(|(owner, _)| *owner == client.id)
                    .count(),
                connected: last_seen
                    .is_some_and(|seen| now.saturating_duration_since(seen) <= CONNECTED),
                id: client.id,
                name: client.name,
                paired_at,
            })
            .collect();
        BridgeStatus {
            port: shared.port,
            error: shared.error.clone(),
            browsers,
        }
    }

    /// The code to pair a browser with. Pairing is open for a few minutes
    /// after each call, so the settings call this while they show the code.
    pub fn pairing_code(&self) -> Option<String> {
        self.shared().pairing.show(Instant::now())
    }

    /// Forgets a paired browser; its token stops working at once.
    pub fn forget(&self, browser: &str) -> bool {
        let mut shared = self.shared();
        shared.forget(browser);
        shared.clients.remove(browser)
    }
}

/// Reads the tabs that paired browsers report.
pub struct BridgeBackend {
    shared: Arc<Mutex<Shared>>,
}

impl BridgeBackend {
    fn shared(&self) -> MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Backend for BridgeBackend {
    fn name(&self) -> &'static str {
        "bridge"
    }

    fn poll(&mut self) -> Vec<Reading> {
        let mut shared = self.shared();
        shared.expire(Instant::now());
        let mut readings: Vec<Reading> = shared
            .tabs
            .iter()
            .map(|((client, tab_id), tab)| {
                protocol::reading(
                    player_id(client, *tab_id),
                    &tab.browser,
                    &tab.update,
                    tab.sampled_at,
                )
            })
            .collect();
        readings.sort_by(|a, b| a.player().id.cmp(&b.player().id));
        readings
    }

    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError> {
        let key = tab_key(player).ok_or(CommandError::PlayerGone)?;
        let mut shared = self.shared();
        let controls = shared
            .tabs
            .get(&key)
            .map(|tab| tab.update.controls)
            .ok_or(CommandError::PlayerGone)?;
        let supported = match command {
            Command::PlayPause | Command::Pause => controls.play_pause,
            Command::Next => controls.next,
            Command::Previous => controls.previous,
            Command::SeekTo { .. } => controls.seek,
        };
        if !supported {
            return Err(CommandError::Unsupported);
        }
        let queue = shared.commands.entry(key).or_default();
        if queue.len() >= MAX_COMMANDS {
            queue.remove(0);
        }
        queue.push((command, Instant::now()));
        Ok(())
    }

    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    fn last_error(&self) -> Option<String> {
        self.shared().error.clone()
    }
}

/// A browser tab that the extension reports may also show up among the
/// system's media sessions. The extension knows the page and the exact
/// position, so the system's copy of the same tab is left out: same song, or
/// the same duration at the same position.
pub fn without_duplicate_tabs(readings: Vec<Reading>) -> Vec<Reading> {
    let now = Instant::now();
    let from_bridge =
        |snapshot: &PlayerSnapshot| snapshot.player.id.as_str().starts_with(PLAYER_PREFIX);
    let tabs: Vec<(String, Option<f64>, Option<f64>)> = readings
        .iter()
        .filter_map(|reading| match reading {
            Reading::Snapshot(snapshot) if from_bridge(snapshot) => snapshot
                .track
                .as_ref()
                .map(|track| (track.key.clone(), track.duration, snapshot.position_at(now))),
            _ => None,
        })
        .collect();
    if tabs.is_empty() {
        return readings;
    }
    readings
        .into_iter()
        .filter(|reading| match reading {
            Reading::Snapshot(snapshot) if snapshot.player.web && !from_bridge(snapshot) => {
                let Some(track) = &snapshot.track else {
                    return true;
                };
                let position = snapshot.position_at(now);
                !tabs.iter().any(|(key, duration, tab_position)| {
                    *key == track.key
                        || matches!(
                            (track.duration, *duration, position, *tab_position),
                            (Some(a), Some(b), Some(p), Some(q)) if (a - b).abs() < 1.0 && (p - q).abs() < 3.0
                        )
                })
            }
            _ => true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Capabilities, PlaybackStatus, PlayerInfo, Service, Track};

    fn snapshot(id: &str, web: bool, key: &str, duration: f64, position: f64) -> Reading {
        Reading::Snapshot(PlayerSnapshot {
            player: PlayerInfo {
                id: PlayerId::new(id),
                name: id.into(),
                service: Service::Other,
                web,
            },
            status: PlaybackStatus::Playing,
            track: Some(Track {
                key: key.into(),
                source_id: None,
                title: key.into(),
                artist: "A".into(),
                album: None,
                duration: Some(duration),
                artwork_url: None,
            }),
            position: Some(position),
            sampled_at: Instant::now(),
            rate: 1.0,
            capabilities: Capabilities::default(),
        })
    }

    fn ids(readings: &[Reading]) -> Vec<&str> {
        readings
            .iter()
            .map(|reading| reading.player().id.as_str())
            .collect()
    }

    #[test]
    fn the_system_copy_of_a_reported_tab_is_left_out() {
        let readings = vec![
            snapshot("mpris:chromium", true, "raw video title", 200.4, 30.2),
            snapshot("bridge:c1:5", true, "song", 200.0, 30.0),
            snapshot("mpris:spotify", false, "song", 200.0, 30.0),
            snapshot("smtc:Chrome", true, "other", 150.0, 10.0),
        ];
        assert_eq!(
            ids(&without_duplicate_tabs(readings)),
            ["bridge:c1:5", "mpris:spotify", "smtc:Chrome"],
            "the desktop app and an unrelated tab stay"
        );
        let same_song = vec![
            snapshot("smtc:Chrome", true, "song", 180.0, 1.0),
            snapshot("bridge:c1:5", true, "song", 200.0, 30.0),
        ];
        assert_eq!(ids(&without_duplicate_tabs(same_song)), ["bridge:c1:5"]);
        let alone = vec![snapshot("mpris:chromium", true, "song", 200.0, 30.0)];
        assert_eq!(ids(&without_duplicate_tabs(alone)), ["mpris:chromium"]);
    }

    #[test]
    fn player_ids_name_browser_and_tab() {
        let id = player_id("0a1b2c3d4e5f6071", 42);
        assert_eq!(id.as_str(), "bridge:0a1b2c3d4e5f6071:42");
        assert_eq!(tab_key(&id), Some(("0a1b2c3d4e5f6071".into(), 42)));
        assert_eq!(
            tab_key(&PlayerId::new("mpris:org.mpris.MediaPlayer2.vlc")),
            None
        );
        assert_eq!(tab_key(&PlayerId::new("bridge:c1:x")), None);
    }
}
