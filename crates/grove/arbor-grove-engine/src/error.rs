//! The engine's error surface.

/// Errors produced by `arbor-grove-engine`.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The audio backend failed (device, decode, …).
    #[error("audio error: {0}")]
    Audio(#[from] arbor_grove_audio::prelude::AudioError),
    /// Writing the offline render failed.
    #[error("render error: {0}")]
    Render(String),
    /// Filesystem / IO failure.
    #[error("io error: {0}")]
    Io(String),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, EngineError>;
