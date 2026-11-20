//! Pairing codes, the browsers that paired, and their tokens.
//!
//! A code is only accepted while the settings show it, and only a few times:
//! after five wrong tries it is replaced. A paired browser gets a random
//! token; only its SHA-256 hash is stored.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How long a code stays valid after the settings last showed it.
pub(crate) const PAIRING_WINDOW: Duration = Duration::from_secs(5 * 60);
const MAX_FAILURES: u8 = 5;
/// More browsers than this, and the one paired longest ago is forgotten.
const MAX_CLIENTS: usize = 8;

/// Random bytes from the operating system.
fn random<const N: usize>() -> Option<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).ok().map(|()| bytes)
}

pub(crate) fn random_hex<const N: usize>() -> Option<String> {
    random::<N>().map(|bytes| hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Six random digits, each value equally likely.
fn random_code() -> Option<String> {
    loop {
        let value = u32::from_le_bytes(random::<4>()?);
        // Values from the top, incomplete million would favor low codes.
        if value < u32::MAX - u32::MAX % 1_000_000 {
            return Some(format!("{:06}", value % 1_000_000));
        }
    }
}

/// Compares in time that does not depend on where the first difference is.
fn same_secret(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

pub(crate) fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairError {
    /// No code is shown right now, or it was used up.
    Closed,
    WrongCode,
}

#[derive(Debug, Default)]
pub(crate) struct Pairing {
    code: Option<String>,
    open_until: Option<Instant>,
    failures: u8,
}

impl Pairing {
    fn is_open(&self, now: Instant) -> bool {
        self.code.is_some() && self.open_until.is_some_and(|until| now < until)
    }

    /// The code to show, valid for [`PAIRING_WINDOW`] from now. A new code is
    /// made when the last one expired or was used up.
    pub fn show(&mut self, now: Instant) -> Option<String> {
        if !self.is_open(now) {
            self.code = random_code();
            self.failures = 0;
        }
        self.open_until = Some(now + PAIRING_WINDOW);
        self.code.clone()
    }

    pub fn try_code(&mut self, code: &str, now: Instant) -> Result<(), PairError> {
        if !self.is_open(now) {
            return Err(PairError::Closed);
        }
        let matches = self
            .code
            .as_deref()
            .is_some_and(|expected| same_secret(expected, code.trim()));
        if matches {
            self.close();
            return Ok(());
        }
        self.failures += 1;
        if self.failures >= MAX_FAILURES {
            self.close();
        }
        Err(PairError::WrongCode)
    }

    fn close(&mut self) {
        self.code = None;
        self.open_until = None;
        self.failures = 0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredClient {
    id: String,
    name: String,
    token_sha256: String,
    /// Seconds since 1970.
    paired_at: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredClients {
    clients: Vec<StoredClient>,
}

/// A browser that paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Client {
    pub id: String,
    pub name: String,
}

/// The paired browsers, kept in a JSON file when there is a path.
#[derive(Debug, Default)]
pub(crate) struct Clients {
    path: Option<PathBuf>,
    clients: Vec<StoredClient>,
    last_seen: HashMap<String, Instant>,
}

impl Clients {
    /// Reads the paired browsers; a missing or broken file means none.
    pub fn load(path: Option<PathBuf>) -> Self {
        let clients = path
            .as_deref()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str::<StoredClients>(&text).ok())
            .map(|stored| stored.clients)
            .unwrap_or_default()
            .into_iter()
            .filter(|client| client.token_sha256.len() == 64)
            .collect();
        Self {
            path,
            clients,
            last_seen: HashMap::new(),
        }
    }

    /// Pairs a browser and returns it with its token, or `None` when the
    /// system has no randomness to offer.
    pub fn add(&mut self, name: &str, paired_at: u64) -> Option<(Client, String)> {
        let id = random_hex::<8>()?;
        let token = random_hex::<32>()?;
        let name: String = name.trim().chars().take(40).collect();
        let name = if name.is_empty() {
            "Browser".to_owned()
        } else {
            name
        };
        while self.clients.len() >= MAX_CLIENTS {
            self.clients.remove(0);
        }
        self.clients.push(StoredClient {
            id: id.clone(),
            name: name.clone(),
            token_sha256: token_hash(&token),
            paired_at,
        });
        self.save();
        Some((Client { id, name }, token))
    }

    /// The browser a request's token belongs to.
    pub fn authenticate(&mut self, token: &str, now: Instant) -> Option<Client> {
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        let hash = token_hash(token);
        let client = self
            .clients
            .iter()
            .find(|client| same_secret(&client.token_sha256, &hash))?;
        self.last_seen.insert(client.id.clone(), now);
        Some(Client {
            id: client.id.clone(),
            name: client.name.clone(),
        })
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.clients.len();
        self.clients.retain(|client| client.id != id);
        self.last_seen.remove(id);
        let removed = self.clients.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// Every paired browser with when it was paired and last heard of.
    pub fn list(&self) -> Vec<(Client, u64, Option<Instant>)> {
        self.clients
            .iter()
            .map(|client| {
                (
                    Client {
                        id: client.id.clone(),
                        name: client.name.clone(),
                    },
                    client.paired_at,
                    self.last_seen.get(&client.id).copied(),
                )
            })
            .collect()
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let stored = StoredClients {
            clients: self.clients.clone(),
        };
        if let Err(error) = write_atomically(path, &stored) {
            log::error!("could not save the paired browsers: {error}");
        }
    }
}

fn write_atomically(path: &Path, stored: &StoredClients) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let temporary = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(stored).map_err(io::Error::other)?;
    fs::write(&temporary, text)?;
    fs::rename(&temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_random_digits() {
        let codes: Vec<String> = (0..50).filter_map(|_| random_code()).collect();
        assert_eq!(codes.len(), 50);
        assert!(codes
            .iter()
            .all(|code| code.len() == 6 && code.bytes().all(|b| b.is_ascii_digit())));
        let distinct: std::collections::HashSet<_> = codes.iter().collect();
        assert!(distinct.len() > 40, "codes repeat too often: {codes:?}");
    }

    #[test]
    fn a_code_works_once_and_only_while_shown() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        assert_eq!(pairing.try_code("123456", now), Err(PairError::Closed));
        let code = pairing.show(now).expect("a code");
        assert_eq!(
            pairing.show(now),
            Some(code.clone()),
            "showing again keeps it"
        );
        assert_eq!(pairing.try_code(&code, now), Ok(()));
        assert_eq!(
            pairing.try_code(&code, now),
            Err(PairError::Closed),
            "used up"
        );

        let code = pairing.show(now).expect("a code");
        let later = now + PAIRING_WINDOW + Duration::from_secs(1);
        assert_eq!(pairing.try_code(&code, later), Err(PairError::Closed));
        assert_ne!(pairing.show(later), None);
    }

    #[test]
    fn five_wrong_tries_replace_the_code() {
        let now = Instant::now();
        let mut pairing = Pairing::default();
        let code = pairing.show(now).expect("a code");
        let wrong = if code == "000000" { "000001" } else { "000000" };
        for _ in 0..MAX_FAILURES {
            assert_eq!(pairing.try_code(wrong, now), Err(PairError::WrongCode));
        }
        assert_eq!(
            pairing.try_code(&code, now),
            Err(PairError::Closed),
            "even the right code no longer works"
        );
    }

    #[test]
    fn tokens_identify_browsers_and_survive_a_restart() {
        let directory =
            std::env::temp_dir().join(format!("singline-bridge-{}", std::process::id()));
        let path = directory.join("browser-bridge.json");
        let now = Instant::now();
        let mut clients = Clients::load(Some(path.clone()));
        let (chrome, token) = clients.add(" Chrome ", 1_800_000_000).expect("paired");
        assert_eq!(chrome.name, "Chrome");
        assert_eq!(token.len(), 64);
        assert_eq!(clients.authenticate(&token, now), Some(chrome.clone()));
        assert_eq!(clients.authenticate(&"0".repeat(64), now), None);
        assert_eq!(clients.authenticate("short", now), None);

        let stored = fs::read_to_string(&path).expect("saved");
        assert!(!stored.contains(&token), "only the hash is stored");
        let mut reloaded = Clients::load(Some(path.clone()));
        assert_eq!(reloaded.authenticate(&token, now), Some(chrome.clone()));
        assert!(reloaded.list()[0].2.is_some(), "last seen");

        assert!(reloaded.remove(&chrome.id));
        assert!(!reloaded.remove(&chrome.id));
        assert_eq!(Clients::load(Some(path.clone())).list().len(), 0);

        fs::write(&path, "{ broken").expect("written");
        assert!(Clients::load(Some(path)).list().is_empty());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn the_oldest_browser_makes_room() {
        let mut clients = Clients::load(None);
        let first = clients.add("First", 1).expect("paired").1;
        for index in 0..MAX_CLIENTS {
            clients.add(&format!("Browser {index}"), 2).expect("paired");
        }
        assert_eq!(clients.list().len(), MAX_CLIENTS);
        assert_eq!(clients.authenticate(&first, Instant::now()), None);
        assert_eq!(clients.add("", 3).expect("paired").0.name, "Browser");
    }
}
