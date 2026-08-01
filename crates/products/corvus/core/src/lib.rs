//! `corvus-core` — the headless backend core for Corvus (the git product).
//!
//! This is the seed of the state that the future `corvus-be` process will own.
//! Today it lives **in-process**: the shell builds one [`CorvusState`] and its
//! `AppState` delegates event egress here. It holds only what has already been
//! made transport-ready (so far: the event sink); it grows field-by-field as the
//! git domains are extracted from the shell (`RepoManager`, `JobRegistry`, …).
//! When `corvus-be` splits into its own binary this struct moves there unchanged
//! and the shell talks to it over [`arbor_ipc`].
//!
//! Tauri-free by construction — it depends only on [`arbor_ipc`] and serde, so
//! the eventual split is `cargo new` + move, not a refactor.
//!
//! ## Public API: use the [`prelude`]

pub mod hooks;
pub mod prelude;
pub mod state;
