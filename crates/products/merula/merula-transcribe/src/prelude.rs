//! Canonical entry point for `merula-transcribe`'s public API.
//!
//! Workspace convention: reach the public surface through `prelude` rather than
//! per-module paths. The submodules stay `pub` for rustdoc navigation only.

// ── Errors ───────────────────────────────────────────────────────────────────
pub use crate::error::{Result, TranscribeError};

// ── The swappable seam + its options/progress ────────────────────────────────
pub use crate::transcriber::{
    ProgressFn, TranscribeOptions, TranscribePhase, TranscribeProgress, Transcriber,
};

// ── Backends + selection ─────────────────────────────────────────────────────
pub use crate::dsp::DspTranscriber;
pub use crate::factory::{transcriber_for, Backend};
#[cfg(feature = "onnx")]
pub use crate::onnx::OnnxTranscriber;

// ── Building blocks (detected notes → MIDI) ──────────────────────────────────
pub use crate::midi_out::notes_to_smf;
pub use crate::note::{DetNote, DRUM_CHANNEL};

// ── Re-exported so callers need one `use` to decode + transcribe ──────────────
pub use merula_audio::prelude::DecodedAudio;
