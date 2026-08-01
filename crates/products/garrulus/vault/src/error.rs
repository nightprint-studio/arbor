//! Failures that can happen while reading or changing a vault.
//!
//! These strings cross the Model-D seam as `Display` output, so they are written
//! for the person who has to fix the vault, not for a log grepper: they name the
//! file, the type or the note that is wrong. Keeping them identical on both sides
//! of the seam is a hard requirement — the string *is* the contract.

use std::path::PathBuf;

use thiserror::Error;

use crate::path::RelPath;

/// Something went wrong reading or interpreting a vault.
#[derive(Debug, Error)]
pub enum VaultError {
    /// The root does not exist, or is not a directory.
    #[error("{} is not a folder", .path.display())]
    NotADirectory { path: PathBuf },

    /// The folder exists but has no `.arbor/garrulus/` marker, so it has never
    /// been opened as a vault.
    #[error("{} is not a Garrulus vault", .path.display())]
    NotAVault { path: PathBuf },

    /// Refusing to create a vault on top of one that already exists — creating
    /// would rewrite `vault.toml` and the built-in types the user has since
    /// edited.
    #[error("{} is already a Garrulus vault", .path.display())]
    AlreadyAVault { path: PathBuf },

    /// A TOML file is there but is not valid, or does not have the shape we
    /// expect. Never fatal to the vault: the caller drops the one file and says
    /// so.
    #[error("{} could not be read: {reason}", .path.display())]
    Malformed { path: PathBuf, reason: String },

    /// The markdown of a note could not be parsed into the document model.
    #[error("{path} could not be parsed: {reason}")]
    Parse { path: RelPath, reason: String },

    /// Reading or writing failed.
    #[error("{}: {reason}", .path.display())]
    Io { path: PathBuf, reason: String },

    /// A note was expected at this path and is not there.
    #[error("the note {path} does not exist")]
    NoteMissing { path: RelPath },

    /// A note is already at the path a create or a rename wants to use.
    /// Overwriting it would lose text the user typed, which this product never
    /// does silently.
    #[error("a note already exists at {path}")]
    NoteExists { path: RelPath },

    /// A path that is not usable as a name on this platform, or that escapes the
    /// vault root.
    #[error("{raw} is not a usable name inside the vault: {reason}")]
    BadPath { raw: String, reason: String },

    /// A type id that no loaded type answers to.
    #[error("there is no note type called `{id}`")]
    UnknownType { id: String },
}

/// The crate's result alias.
pub type VaultResult<T> = std::result::Result<T, VaultError>;

impl VaultError {
    /// Build an [`VaultError::Io`] from a path and an underlying failure.
    ///
    /// Every I/O call in this crate goes through here so the message shape is
    /// decided once rather than at forty call sites.
    pub fn io(path: impl Into<PathBuf>, source: impl std::fmt::Display) -> Self {
        VaultError::Io { path: path.into(), reason: source.to_string() }
    }

    /// Build an [`VaultError::Malformed`] from a path and a parse failure.
    pub fn malformed(path: impl Into<PathBuf>, source: impl std::fmt::Display) -> Self {
        VaultError::Malformed { path: path.into(), reason: source.to_string() }
    }
}
