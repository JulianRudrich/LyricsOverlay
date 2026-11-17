use std::time::Duration;

use thiserror::Error;

use crate::model::{Command, PlayerId, Reading};

/// One way of finding players, such as MPRIS on Linux.
pub trait Backend: Send {
    /// Short name for diagnostics, such as `mpris`.
    fn name(&self) -> &'static str;

    /// Reads every open player. Must never start a player that is not running.
    /// Players that cannot be read are reported as [`Reading::Unavailable`].
    fn poll(&mut self) -> Vec<Reading>;

    /// Sends a transport command to one player.
    fn send(&mut self, player: &PlayerId, command: Command) -> Result<(), CommandError>;

    /// A good pause between two polls for this backend.
    fn poll_interval(&self) -> Duration {
        Duration::from_millis(500)
    }

    /// Why the last poll found nothing, when the backend itself failed.
    fn last_error(&self) -> Option<String> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandError {
    #[error("the player is no longer open")]
    PlayerGone,
    #[error("the player does not support this command")]
    Unsupported,
    #[error("the command failed: {0}")]
    Failed(String),
}

/// The backends for the system the app runs on.
pub fn default_backends() -> Vec<Box<dyn Backend>> {
    #[cfg(target_os = "linux")]
    {
        vec![Box::new(crate::mpris::MprisBackend::session())]
    }
    #[cfg(windows)]
    {
        vec![Box::new(crate::smtc::SmtcBackend::new())]
    }
    #[cfg(target_os = "macos")]
    {
        vec![Box::new(crate::applescript::AppleScriptBackend::new())]
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        Vec::new()
    }
}
