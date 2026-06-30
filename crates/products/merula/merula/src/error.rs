//! The facade's **unified** error type.
//!
//! Each merula crate has its own `Result<T, …Error>` alias; glob-merging all four
//! preludes would collide on the bare `Result` name. Rather than push that
//! collision onto consumers, the facade exposes a single [`MerulaError`] that
//! wraps the four crate errors (each via `#[from]`, so `?` just works) and a
//! matching [`Result`] alias. This is also exactly what the Arbor shell wants: a
//! single merula error to convert into `AppError` at the IPC boundary.

use thiserror::Error;

use merula_audio::prelude::AudioError;
use merula_engine::prelude::EngineError;
use merula_import::prelude::ImportError;
use merula_lang::prelude::LangError;
use merula_pattern::prelude::PatternError;
use merula_transcribe::prelude::TranscribeError;

/// Any failure surfaced through the merula facade.
///
/// The variants mirror the four crates. Conversions are wired with `#[from]`, so
/// a `PatternError`/`LangError`/`AudioError`/`EngineError` propagates into a
/// `MerulaError` automatically under `?`.
#[derive(Debug, Error)]
pub enum MerulaError {
    /// A pure-algebra failure (scale/note parsing).
    #[error("pattern error: {0}")]
    Pattern(#[from] PatternError),
    /// A language failure (parse / eval / import), carries its source span.
    #[error("language error: {0}")]
    Lang(#[from] LangError),
    /// An audio-backend failure (device, decode, registry, IO).
    #[error("audio error: {0}")]
    Audio(#[from] AudioError),
    /// An engine failure (offline render / IO). Note `EngineError` already wraps
    /// `AudioError`; a bare `AudioError` maps to [`MerulaError::Audio`] instead.
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
    /// A deterministic MIDI → `.merula` import failure (bad MIDI, no notes).
    #[error("import error: {0}")]
    Import(#[from] ImportError),
    /// A WAV → MIDI transcription failure (decode, no content, backend).
    #[error("transcribe error: {0}")]
    Transcribe(#[from] TranscribeError),
}

/// The facade's result alias — the single `Result` the prelude exposes in place
/// of the four per-crate aliases.
pub type Result<T> = std::result::Result<T, MerulaError>;
