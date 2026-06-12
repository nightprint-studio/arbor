//! # arbor-nemus-import
//!
//! The **deterministic** half of nemus's audio/MIDI import: turn a MIDI file
//! into idiomatic `.nemus` source, with **no AI** anywhere in the path. (The
//! AI half — WAV → MIDI transcription — is [`arbor-nemus-transcribe`]; this
//! crate begins once a MIDI byte stream exists, whether it came off disk or was
//! produced in memory by the transcriber.)
//!
//! ```text
//! MIDI bytes ──L1 transcode──▶ Song (notes in cycle-time)
//!                              │
//!              ┌──────────────┘ L2 heuristics
//!              ▼
//!   quantize · key/scale detection · chord grouping · loop detection
//!              │
//!              ▼ emit (build arbor-nemus-lang AST, print canonically)
//!   idiomatic .nemus source
//! ```
//!
//! ## Layers (each independently testable)
//!
//! - [`transcode`] — **L1**: MIDI events → a neutral [`Song`](model::Song) of
//!   explicit notes (pitch, duration, velocity, timing) measured in *cycles*.
//! - [`quantize`] — **L2**: snap onsets/durations to a configurable grid.
//! - [`key`] — **L2**: pitch-class histogram → best-fit scale (incl. the
//!   non-Western modes the author uses, e.g. *hirajoshi*, *in-sen*).
//! - [`chords`] — **L2**: a set of simultaneous notes → a chord symbol, via the
//!   language's own chord table (single source of truth).
//! - [`emit`] — assembles the idiomatic output: scales/degrees, chord symbols,
//!   per-cycle loop collapse, and **phrase factoring** (repeated sections become
//!   `let` bindings played through `arrange(section(...))`, so a long take reads
//!   as a handful of named phrases), printed through `arbor-nemus-lang`'s emitter.
//!
//! The one-call entry point is [`midi_to_nemus`](convert::midi_to_nemus).
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).
//!
//! [`arbor-nemus-transcribe`]: https://docs.rs/arbor-nemus-transcribe

mod build;
pub mod chords;
pub mod convert;
pub mod emit;
pub mod error;
pub mod gm_drum;
pub mod key;
pub mod model;
pub mod prelude;
pub mod quantize;
pub mod transcode;
