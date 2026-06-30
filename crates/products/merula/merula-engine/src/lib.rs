//! # merula-engine
//!
//! The **timing runtime** of merula (Arbor's live-coding music engine): it turns
//! patterns into sound *at the right moment*. It sits between the pure algebra
//! and the audio backend, depending on both — but **not** on `merula-lang`
//! (the shell evaluates source → [`Tracks`](merula_pattern::prelude::Tracks)
//! and hands those in; the engine speaks only `Pattern`/`ControlMap` + the audio
//! seam).
//!
//! ## Pieces
//!
//! - [`clock`] — the [`Epoch`](prelude::Epoch) mapping cycle-time ↔ output
//!   frames (`frames_per_cycle = sample_rate / cps`); audio owns the sample
//!   clock, the engine owns `cps`.
//! - [`schedule`] — [`schedule_span`](prelude::schedule_span), the **pure**
//!   look-ahead core shared by live and offline.
//! - [`transport`] — [`Transport`](prelude::Transport), the real-time driver
//!   over an [`AudioSink`](merula_audio::prelude::AudioSink), with quantized
//!   tempo / re-eval swaps.
//! - [`render`] — [`render_offline`](prelude::render_offline), the non-real-time
//!   driver that reuses the same scheduling + `Renderer` and writes WAV.
//!
//! Testable headless: drive a [`Transport`](prelude::Transport) with
//! `merula_audio::prelude::RecordingSink` and assert on the emitted events
//! — no device, no real time.
//!
//! ## Entry point
//!
//! Reach the public API through [`prelude`] (workspace convention).

pub mod clock;
pub mod encode;
pub mod error;
pub mod midi;
pub mod prelude;
pub mod render;
pub mod schedule;
pub mod transport;
