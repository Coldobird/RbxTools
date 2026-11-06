//! The one error type of the backend: a kind the interface can react to, and a
//! message written for the user.

use serde::Serialize;
use std::fmt;

/// What kind of failure occurred.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// Anything to show the user as is.
    Failed,
    /// The same work is still running; try again shortly.
    Busy,
    /// Steam ignored a graceful shutdown; a forced restart may be offered.
    SteamStillRunning,
}

/// A failure with a user-facing message. Commands return it to the interface
/// as `{ kind, message }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

impl Error {
    pub fn busy(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Busy,
            message: message.into(),
        }
    }

    pub fn steam_still_running() -> Self {
        Self {
            kind: ErrorKind::SteamStillRunning,
            message: "Steam is still running.".into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            kind: ErrorKind::Failed,
            message,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::from(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::from(error.to_string())
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}
