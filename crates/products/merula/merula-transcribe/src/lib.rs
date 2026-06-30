//! # merula-transcribe
//!
//! WAV → MIDI transcription for merula, behind a swappable [`Transcriber`] trait
//! so the implementation can be replaced without touching callers (D1's
//! requirement). The crate ships one always-available backend — [`DspTranscriber`],
//! a zero-dependency DSP transcriber — and is the home for ML backends
//! (basic-pitch for pitched material, Demucs for stem separation) which plug into
//! the same trait once their ONNX models are present.
//!
//! ```text
//! DecodedAudio ──Transcriber──▶ midly::Smf
//!                               │
//!                  ┌────────────┴───────────┐
//!                  ▼                         ▼
//!   in memory → merula-import      written to disk → .mid
//!   (transient WAV → .merula)            ("Convert WAV to MIDI")
//! ```
//!
//! ## Backends
//!
//! - [`DspTranscriber`] — **built-in, no extra deps**: monophonic pitch via YIN
//!   ([`dsp::pitch`]) and drums via energy-onset detection mapped to GM keys
//!   ([`dsp::onset`]). Fast and rough — the baseline that always works offline,
//!   even with no models installed. Stem separation is a no-op here (it runs on
//!   the mix).
//! - **ONNX backends (basic-pitch, Demucs)** — a future [`Transcriber`] impl whose
//!   models download on demand (reusing merula's sample-pack download path). The
//!   trait is the seam; nothing else changes when they land.
//!
//! Pick a backend through [`transcriber_for`]. The interchange is always a
//! [`midly::Smf`] — the deterministic `.merula` converter (`merula-import`)
//! consumes it directly, in memory; the "convert to .mid" path writes it to disk.
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).

pub mod dsp;
pub mod error;
pub mod factory;
pub mod midi_out;
pub mod note;
#[cfg(feature = "onnx")]
pub mod onnx;
pub mod prelude;
pub mod transcriber;
