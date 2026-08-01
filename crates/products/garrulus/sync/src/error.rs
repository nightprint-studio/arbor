//! [`SyncError`] — one error for the whole seam.
//!
//! The strings are developer-facing English; the *user-facing* Italian lives in
//! the strings the engine writes into the vault (commit messages, conflict side
//! file names), because those end up in a git log and in a file listing where
//! the user reads them.

use thiserror::Error;

/// Failures of a sync operation.
#[derive(Debug, Error)]
pub enum SyncError {
    /// The local filesystem refused.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Anything the git engine reported (libgit2 or the `git` binary).
    ///
    /// Deliberately a `String`: `corvus-git`'s `GitError` is not part of this
    /// crate's public vocabulary — `FolderRemote` has no git in it at all.
    #[error("Git error: {0}")]
    Git(String),

    /// The remote is configured but could not be reached or authenticated.
    /// Distinguished from [`SyncError::Git`] so the UI can stay quiet and retry
    /// (`docs/garrulus-design.md` §4.2, auto-reconnect rules).
    #[error("Remote unreachable: {0}")]
    Offline(String),

    /// The remote is missing, half-configured, or points somewhere that is not
    /// there any more.
    #[error("Remote not configured: {0}")]
    NotConfigured(String),

    /// The operation is not supported by this kind of remote — check
    /// [`crate::remote::RemoteCapabilities`] before calling.
    #[error("Not supported by this remote: {0}")]
    Unsupported(&'static str),

    /// The blocking worker that was doing the work went away.
    #[error("Sync task failed: {0}")]
    Task(String),
}

/// Result alias used across the seam.
pub type SyncResult<T> = std::result::Result<T, SyncError>;

impl SyncError {
    /// Classify a git failure message as *unreachable* rather than *broken*.
    ///
    /// Worth the fuzzy matching: the difference decides whether the UI shows a
    /// silent retry banner or a red error, and libgit2/`git` report a dead
    /// network as a dozen different sentences with no code attached.
    pub fn from_git_message(msg: impl Into<String>) -> Self {
        let msg = msg.into();
        if is_offline_message(&msg) {
            SyncError::Offline(msg)
        } else {
            SyncError::Git(msg)
        }
    }
}

/// Does this git error text describe a network/auth reachability problem?
pub fn is_offline_message(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    const NEEDLES: &[&str] = &[
        "could not resolve host",
        "couldn't resolve host",
        "failed to connect",
        "connection refused",
        "connection reset",
        "connection timed out",
        "network is unreachable",
        "no route to host",
        "temporary failure in name resolution",
        "operation timed out",
        "ssl connect error",
        "unexpected disconnect",
        "early eof",
        "remote end hung up",
    ];
    NEEDLES.iter().any(|n| m.contains(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_sentences_are_offline() {
        assert!(is_offline_message(
            "fatal: unable to access 'https://x/': Could not resolve host: github.com"
        ));
        assert!(is_offline_message("Failed to connect to github.com port 443"));
        assert!(is_offline_message("early EOF"));
    }

    #[test]
    fn real_git_failures_are_not_offline() {
        assert!(!is_offline_message("remote rejected push of 'refs/heads/main': protected"));
        assert!(!is_offline_message("Authentication failed"));
    }

    #[test]
    fn classification_picks_the_variant() {
        assert!(matches!(
            SyncError::from_git_message("Connection refused"),
            SyncError::Offline(_)
        ));
        assert!(matches!(
            SyncError::from_git_message("bad object HEAD"),
            SyncError::Git(_)
        ));
    }
}
