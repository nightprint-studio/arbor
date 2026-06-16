//! [`FsError`] — the error type for pure filesystem operations.
//!
//! Variants are shaped so the host shell can map them back to its `AppError`
//! with the **exact same wire string** the explorer showed before the split
//! (see `From<FsError> for AppError` in the shell): IO failures keep their
//! human action context, cancellation matches the host `Cancelled` message, and
//! the pre-formatted string variants pass through untouched.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FsError {
    /// An IO failure carrying the human action context the explorer surfaces
    /// (e.g. `"Cannot copy C:\\a\\b"`). Built via [`FsError::io`].
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },

    /// A user-cancelled long-running operation. Its `Display` matches the host's
    /// `AppError::Cancelled` message so the wire string is unchanged.
    #[error("Operation cancelled")]
    Cancelled,

    /// A validation / precondition failure with a ready-to-show message
    /// (invalid path, "move a folder into itself", duplicate rename targets…).
    #[error("{0}")]
    Invalid(String),

    /// A trash / Recycle Bin backend failure (already message-formatted).
    #[error("{0}")]
    Trash(String),

    /// A ZIP archive failure (already message-formatted).
    #[error("{0}")]
    Zip(String),

    /// The operation isn't supported on this platform (already message-formatted).
    #[error("{0}")]
    Unsupported(String),
}

impl FsError {
    /// Build an [`FsError::Io`] from an action context and the underlying error.
    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        FsError::Io { context: context.into(), source }
    }
}

pub type Result<T> = std::result::Result<T, FsError>;
