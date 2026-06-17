//! [`GitError`] — the crate's error, mapped by each consumer to its own type.

use thiserror::Error;

/// Failures from local-git operations. The shell maps it to `AppError`
/// variant-for-variant so the frontend wire string is unchanged from before the
/// extraction (`Git` → `AppError::Git`, `Io` → `AppError::Io`, `StashNotFound`
/// → `AppError::StashNotFound`, `Other` → `AppError::Other`). The `Display`
/// strings match `AppError`'s exactly so the out-of-process path (which crosses
/// the error as its `Display` string) is byte-identical too.
#[derive(Debug, Error)]
pub enum GitError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Stash not found at index {0}")]
    StashNotFound(usize),

    #[error("{0}")]
    Other(String),
}

/// Convenience alias used inside the crate.
pub type Result<T> = std::result::Result<T, GitError>;
