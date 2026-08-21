//! Plugin & theme marketplace — catalog resolution, installer, cache.
//!
//! Resolves entries from the curated `arbor-extensions` GitHub repo plus any
//! user-added custom sources (`user_registry.toml`), installs them via zipball
//! extraction, and tracks state on disk in `marketplace_installed.json` +
//! `marketplace_cache.json`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public surface
//! through a `prelude` module. Consumers either glob-import it once per file
//! (`use arbor_plugin_marketplace::prelude::*;`) or fully qualify
//! (`arbor_plugin_marketplace::prelude::MarketplaceRegistry`). The per-feature
//! submodules stay `pub` for rustdoc navigation, but call sites should go
//! through the prelude.
//!
//! ## Host coupling
//!
//! The marketplace is intentionally Tauri-agnostic. Anything the registry +
//! installer need from the host process (which plugins live on disk, what
//! their enable state is, where the dev plugin folder is) is reached through
//! the [`host::MarketplaceHost`] trait. The Tauri shell implements it once;
//! tests use a small mock.
//!
//! See `docs/crate-refactor.md` for the full split plan.

pub mod cache;
pub mod custom;
pub mod error;
pub mod fetch;
pub mod github_api;
pub mod host;
pub mod index;
pub mod integrity;
pub mod installer;
pub mod installs;
pub mod paths;
pub mod prelude;
pub mod refresh;
pub mod registry;
pub mod types;
pub mod user_registry;
