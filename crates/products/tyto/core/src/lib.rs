//! `tyto-core` — the headless backend core for Tyto (the screen-recorder product).
//!
//! The tyto twin of `sitta-core` / `merula-core`: the canonical [`TytoState`] the
//! `tyto-be` process owns, **Tauri-free by construction**. Deliberately small — a
//! recorder's heavy lifting (screen capture, audio, encoding) lives in the
//! recording engine the tyto-be domain handlers drive; this state holds only the
//! BE→FE event egress + the reverse channel back to the shell. Every field a later
//! wave needs already lives here, so a wave fills in handlers against these
//! accessors and never has to re-edit this file.
//!
//! Modules here depend only on [`arbor_ipc`], `arbor_core` (path resolution for
//! the typed config) and serde — no `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites (in `tyto-be`) reach this crate's surface
//! through `tyto_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod config;
pub mod prelude;
pub mod state;
