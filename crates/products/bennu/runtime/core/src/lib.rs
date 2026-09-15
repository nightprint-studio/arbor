//! `bennu-core` — the headless backend core for Bennu (the Java editor / analysis
//! product).
//!
//! The bennu twin of `tyto-core` / `sitta-core`: the canonical [`BennuState`] the
//! `bennu-be` process owns, **Tauri-free by construction**. Deliberately small — a
//! Java analyzer's heavy lifting (bytecode reading, the symbol index, tree-sitter
//! parsing, capability detection) lives in the leaf analysis crates (`bennu-index`,
//! `bennu-classpath`, `bennu-project`, …) the domain handlers drive; this state
//! holds only the BE→FE event egress + the reverse channel back to the shell (for
//! host round-trips like reveal-in-explorer / open-path). Every field a later wave
//! needs already lives here, so a wave fills in handlers against these accessors and
//! never has to re-edit this file.
//!
//! Modules here depend only on [`arbor_ipc`], `arbor_core` (path resolution for the
//! typed config) and serde — no `tauri`, no `arbor_rpc`, no `arbor_be`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites (in `bennu-be`) reach this crate's surface
//! through `bennu_core::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the prelude is the canonical call-site path.

pub mod config;
pub mod prelude;
pub mod state;
