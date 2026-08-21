//! Pure data shapes shared by the plugin runtime and the marketplace.
//!
//! No runtime behaviour, no mlua, no Tauri — just the `plugin.toml` shape, the
//! permission tiers, the schedule descriptors, the dependency graph atoms, and
//! the canonical hook catalog (names + ctx schemas).
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public surface
//! through a `prelude` module. Consumers either glob-import it once per file
//! (`use arbor_plugin_types::prelude::*;`) or fully qualify
//! (`arbor_plugin_types::prelude::Manifest`). The per-feature submodules
//! (`manifest`, `permissions`, …) stay `pub` for discoverability and rustdoc
//! navigation, but call sites should go through the prelude so a single glob
//! import is enough.
//!
//! See `docs/crate-refactor.md` for the full split plan.

pub mod credentials;
pub mod dependency;
pub mod hook_catalog;
pub mod hook_names;
pub mod hook_ns;
pub mod hooks;
pub mod manifest;
pub mod network;
pub mod permissions;
pub mod provides;
pub mod prelude;
pub mod sandbox;
pub mod schedule;
