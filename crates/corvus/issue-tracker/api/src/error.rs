//! [`IssueTrackerError`] — failures a tracker impl surfaces.
//!
//! Kept message-carrying so the shell maps it onto `AppError` byte-identically
//! (the FE matches/displays these strings): `Auth`/`NotConnected` → the auth
//! string, the rest → the generic error string.

use std::fmt;

/// A tracker operation failure.
#[derive(Debug, Clone)]
pub enum IssueTrackerError {
    /// No credential is configured for this tracker.
    NotConnected(String),
    /// The provider rejected the credential (401/403), even after a refresh.
    Auth(String),
    /// The provider returned an application-level error (GraphQL/REST error).
    Api(String),
    /// The request never completed (transport/JSON decode).
    Network(String),
}

impl IssueTrackerError {
    /// The human-readable message, without the variant tag.
    pub fn message(&self) -> &str {
        match self {
            Self::NotConnected(m) | Self::Auth(m) | Self::Api(m) | Self::Network(m) => m,
        }
    }
}

impl fmt::Display for IssueTrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for IssueTrackerError {}

pub type Result<T> = std::result::Result<T, IssueTrackerError>;
