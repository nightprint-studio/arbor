//! What can go wrong in a content store.

/// A local-history failure.
///
/// Deliberately small: almost everything here is either "the filesystem said no" or
/// "the log says something this code does not understand", and a caller can do the same
/// thing about both — report it and carry on **without** taking the user's edit with it.
/// History is a safety net; a safety net that can fail a save is worse than none.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// A path outside the project the store was opened for. Not an accident to paper
    /// over: a store keyed by one root must never write another root's files.
    #[error("{0} is outside the project")]
    Outside(String),
    #[error("revision {0} not found")]
    NoRevision(String),
    /// The revision exists but has no content — a deletion marks the end of a file's
    /// life and stores no bytes.
    #[error("revision {0} has no content (it records a deletion)")]
    NoContent(String),
}

pub type HistoryResult<T> = Result<T, HistoryError>;
