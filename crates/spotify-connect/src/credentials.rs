//! Where the refresh token lives: the operating system's credential store
//! (Windows Credential Manager, the macOS Keychain, the Secret Service on
//! Linux), or only memory when there is none. The access token never comes
//! here; it stays in memory.

use std::sync::Mutex;

use thiserror::Error;

/// The service name of the stored login.
pub const SERVICE: &str = "Singline";
/// The account name of the stored login.
pub const ACCOUNT: &str = "spotify-refresh-token";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CredentialError {
    /// There is no credential store, or it refused access, for example
    /// because no Secret Service runs on this Linux desktop.
    #[error("the system's credential store is not available: {0}")]
    Unavailable(String),
}

/// Keeps one secret. Calls may block, for example while the system asks to
/// unlock the keyring, so async code runs them on a blocking thread.
pub trait CredentialStore: Send + Sync {
    fn load(&self) -> Result<Option<String>, CredentialError>;
    fn save(&self, secret: &str) -> Result<(), CredentialError>;
    fn delete(&self) -> Result<(), CredentialError>;
}

/// The operating system's credential store.
#[derive(Debug, Clone)]
pub struct SystemCredentials {
    service: String,
    account: String,
}

impl SystemCredentials {
    pub fn new(service: &str, account: &str) -> Self {
        Self {
            service: service.to_owned(),
            account: account.to_owned(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(&self.service, &self.account).map_err(unavailable)
    }
}

impl Default for SystemCredentials {
    fn default() -> Self {
        Self::new(SERVICE, ACCOUNT)
    }
}

impl CredentialStore for SystemCredentials {
    fn load(&self) -> Result<Option<String>, CredentialError> {
        match self.entry()?.get_password() {
            Ok(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            // A garbled entry is no login; connecting again overwrites it.
            Err(keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..)) => Ok(None),
            Err(error) => Err(unavailable(error)),
        }
    }

    fn save(&self, secret: &str) -> Result<(), CredentialError> {
        self.entry()?.set_password(secret).map_err(unavailable)
    }

    fn delete(&self) -> Result<(), CredentialError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(unavailable(error)),
        }
    }
}

/// Describes a store error. Variants that carry the stored bytes are named
/// only, so a secret can never end up in a message.
fn unavailable(error: keyring::Error) -> CredentialError {
    let message = match &error {
        keyring::Error::BadEncoding(_) | keyring::Error::BadDataFormat(..) => {
            "the stored login is unreadable".to_owned()
        }
        keyring::Error::Ambiguous(_) => "the stored login exists more than once".to_owned(),
        other => other.to_string(),
    };
    CredentialError::Unavailable(message)
}

/// Keeps the secret only while the app runs: for tests, the demo, and as the
/// fallback when the system has no credential store.
#[derive(Debug, Default)]
pub struct MemoryCredentials {
    secret: Mutex<Option<String>>,
}

impl MemoryCredentials {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_secret(secret: &str) -> Self {
        Self {
            secret: Mutex::new(Some(secret.to_owned())),
        }
    }
}

impl CredentialStore for MemoryCredentials {
    fn load(&self) -> Result<Option<String>, CredentialError> {
        Ok(self
            .secret
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone())
    }

    fn save(&self, secret: &str) -> Result<(), CredentialError> {
        *self.secret.lock().unwrap_or_else(|p| p.into_inner()) = Some(secret.to_owned());
        Ok(())
    }

    fn delete(&self) -> Result<(), CredentialError> {
        *self.secret.lock().unwrap_or_else(|p| p.into_inner()) = None;
        Ok(())
    }
}

/// A store that is never available, for tests of the fallback.
#[derive(Debug, Default)]
pub struct UnavailableCredentials;

impl CredentialStore for UnavailableCredentials {
    fn load(&self) -> Result<Option<String>, CredentialError> {
        Err(CredentialError::Unavailable("no credential store".into()))
    }

    fn save(&self, _secret: &str) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable("no credential store".into()))
    }

    fn delete(&self) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable("no credential store".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_keeps_one_secret() {
        let store = MemoryCredentials::new();
        assert_eq!(store.load(), Ok(None));
        store.save("first").expect("saved");
        store.save("second").expect("saved");
        assert_eq!(store.load(), Ok(Some("second".into())));
        store.delete().expect("deleted");
        store.delete().expect("deleting twice is fine");
        assert_eq!(store.load(), Ok(None));
        assert_eq!(
            MemoryCredentials::with_secret("x").load(),
            Ok(Some("x".into()))
        );
    }

    #[test]
    fn errors_never_show_stored_bytes() {
        let secret = b"AQD-refresh-secret".to_vec();
        let message = unavailable(keyring::Error::BadEncoding(secret)).to_string();
        assert!(!message.contains("AQD"), "{message}");
        let message = unavailable(keyring::Error::BadDataFormat(
            b"AQD-refresh-secret".to_vec(),
            "broken".into(),
        ))
        .to_string();
        assert!(!message.contains("AQD"), "{message}");
        assert!(unavailable(keyring::Error::NoDefaultStore)
            .to_string()
            .starts_with("the system's credential store is not available"));
    }

    /// Writes to the real credential store of this computer, so it runs
    /// only on request: `cargo test -p spotify-connect -- --ignored`.
    #[test]
    #[ignore = "uses the system's credential store"]
    fn system_store_round_trip() {
        let store = SystemCredentials::new("Singline test", "round-trip");
        match store.save("secret value") {
            Ok(()) => {
                assert_eq!(store.load(), Ok(Some("secret value".into())));
                store.delete().expect("deleted");
                assert_eq!(store.load(), Ok(None));
            }
            Err(CredentialError::Unavailable(reason)) => {
                eprintln!("no credential store here: {reason}");
                assert!(store.load().is_err() || store.load() == Ok(None));
            }
        }
    }
}
