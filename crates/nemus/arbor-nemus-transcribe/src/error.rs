//! The transcribe crate's error surface.

/// Errors produced by `arbor-nemus-transcribe`.
#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    /// Decoding the input audio failed.
    #[error("audio decode error: {0}")]
    Audio(#[from] arbor_nemus_audio::prelude::AudioError),
    /// The transcriber ran but found nothing to write (silence / too short).
    #[error("no transcribable content found in the audio")]
    NoContent,
    /// A backend failed mid-transcription.
    #[error("transcription backend failed: {0}")]
    Backend(String),
    /// The requested backend isn't available (e.g. an ML model isn't installed).
    #[error("transcription backend unavailable: {0}")]
    Unavailable(String),
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, TranscribeError>;
