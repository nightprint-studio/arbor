//! The import crate's error surface.

/// Errors produced by `merula-import`.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// The MIDI byte stream could not be parsed.
    #[error("invalid MIDI file: {0}")]
    Midi(String),
    /// The MIDI uses a timing mode we don't support (SMPTE timecode); only
    /// metrical (ticks-per-quarter) timing is handled.
    #[error("unsupported MIDI timing (SMPTE timecode); only metrical timing is supported")]
    UnsupportedTiming,
    /// The MIDI parsed but carried no usable note events.
    #[error("the MIDI file contains no notes")]
    Empty,
}

/// Crate-local result alias.
pub type Result<T> = std::result::Result<T, ImportError>;
