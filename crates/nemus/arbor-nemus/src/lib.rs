//! # arbor-nemus
//!
//! The **facade** of nemus (Arbor's live-coding music engine). nemus is split
//! into four library crates with one-way dependencies:
//!
//! ```text
//!         arbor-nemus-pattern        pure pattern algebra (zero deps)
//!          ↑               ↑
//! arbor-nemus-lang   arbor-nemus-engine ← arbor-nemus-audio
//! ```
//!
//! Each exposes its API through its own `prelude`. This crate stitches those
//! four preludes into **one** curated [`prelude`], so the Arbor shell writes a
//! single `use arbor_nemus::prelude::*;` instead of importing from four crates.
//!
//! ## What the facade actually does
//!
//! The four crates' preludes very nearly glob-merge cleanly — the public surface
//! is mostly single-homed, and the few shared types (`ControlMap`, `Pattern`,
//! `SourceKind`, `TimeSpan`, …) are *identical* re-exports that Rust deduplicates.
//! The **only** real collision is the per-crate `Result` alias (four distinct
//! `Result<T, …Error>`). The facade resolves it by curating the prelude: it
//! re-exports every public item by name **except** the four `Result` aliases, and
//! offers a single unified [`NemusError`](error::NemusError) +
//! [`Result`](error::Result) in their place. The four underlying error types stay
//! reachable by their own names (`PatternError`, `LangError`, `AudioError`,
//! `EngineError`) and convert into `NemusError` with `?`.
//!
//! ## Entry point
//!
//! Reach the whole nemus API through [`prelude`] (workspace convention). There
//! are deliberately **no** per-crate namespaced modules here — the prelude is the
//! one canonical surface.

pub mod error;
pub mod prelude;
