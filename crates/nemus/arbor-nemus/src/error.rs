//! The facade's **unified** error type.
//!
//! Each nemus crate has its own `Result<T, …Error>` alias; glob-merging all four
//! preludes would collide on the bare `Result` name. Rather than push that
//! collision onto consumers, the facade exposes a single [`NemusError`] that
//! wraps the four crate errors (each via `#[from]`, so `?` just works) and a
//! matching [`Result`] alias. This is also exactly what the Arbor shell wants: a
//! single nemus error to convert into `AppError` at the IPC boundary.

use thiserror::Error;

use arbor_nemus_audio::prelude::AudioError;
use arbor_nemus_engine::prelude::EngineError;
use arbor_nemus_lang::prelude::LangError;
use arbor_nemus_pattern::prelude::PatternError;

/// Any failure surfaced through the nemus facade.
///
/// The variants mirror the four crates. Conversions are wired with `#[from]`, so
/// a `PatternError`/`LangError`/`AudioError`/`EngineError` propagates into a
/// `NemusError` automatically under `?`.
#[derive(Debug, Error)]
pub enum NemusError {
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
    /// `AudioError`; a bare `AudioError` maps to [`NemusError::Audio`] instead.
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
}

/// The facade's result alias — the single `Result` the prelude exposes in place
/// of the four per-crate aliases.
pub type Result<T> = std::result::Result<T, NemusError>;
