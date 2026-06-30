//! `sitta-core` — the headless backend core for Sitta (the file-explorer product).
//!
//! The sitta twin of `corvus-core` / `merula-core`: the canonical [`SittaState`]
//! the `sitta-be` process owns, **Tauri-free by construction**. Deliberately tiny —
//! a file manager's heavy lifting is filesystem I/O (already in `arbor-fs`, served
//! today by the shell's `platform` broker) and git-awareness (in `corvus-git`), so
//! this state holds only the event egress + the reverse channel back to the shell.
//! Every field a later wave needs already lives here, so a wave fills in handlers
//! against these accessors and never has to re-edit this file.
//!
//! Modules here depend only on [`arbor_ipc`], `arbor_core` (path resolution for
//! the typed config) and serde — no `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites (in `sitta-be`) reach this crate's surface
//! through `sitta_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod config;
pub mod prelude;
pub mod state;
