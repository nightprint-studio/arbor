//! `voice-core` — the product-agnostic core of local voice dictation.
//!
//! Everything here is pure (or a thin file/IPC wrapper) so it can be unit-tested without a
//! microphone or a model: the heavy parts (cpal capture, whisper.cpp) live in `voice-be`.
//!
//! Call sites use the [`prelude`].

pub mod audio;
pub mod bench;
pub mod config;
pub mod models;
pub mod plan;
pub mod prelude;
pub mod selection;
pub mod speech;
pub mod stream;
pub mod state;
pub mod vocab;
pub mod wake;
