//! The audio crate's error surface.
//!
//! Kept local (workspace rule: a library crate leans only on its own `error`
//! plus base types, so it can be split off cleanly later).

/// Errors produced by `arbor-grove-audio`.
#[derive(Debug, thiserror::Error)]
pub enum AudioError {
    /// The output device or stream could not be opened / configured.
    #[error("audio device error: {0}")]
    Device(String),
    /// A media file could not be decoded.
    #[error("failed to decode {path}: {reason}")]
    Decode { path: String, reason: String },
    /// An `.sfz` instrument definition could not be parsed.
    #[error("sfz parse error in {path}: {reason}")]
    Sfz { path: String, reason: String },
    /// A sound/instrument name did not resolve in the registry.
    #[error("unknown instrument or sound: {0}")]
    UnknownVoice(String),
    /// Filesystem / IO failure.
    #[error("io error: {0}")]
    Io(String),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, AudioError>;
