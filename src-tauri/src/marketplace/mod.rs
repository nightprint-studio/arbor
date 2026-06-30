//! Tauri-side glue for the plugin & theme marketplace.
//!
//! The catalog, cache, installer, and registry now live in
//! `arbor-plugin-marketplace` (see `crates/platform/plugin/marketplace`). This module
//! only carries the shell-coupled bits that crate can't host:
//!
//!   * [`scheduler`] — wires the auto-refresh into `arbor-scheduler` using
//!     the live `AppHandle` + `AppState`.
//!   * [`TauriMarketplaceHost`] — implements
//!     [`arbor_plugin_marketplace::prelude::MarketplaceHost`] on top of
//!     `arbor_plugin_core` (which the marketplace crate doesn't depend on).
//!   * [`build_registry`] — convenience constructor used at app startup.
//!
//! Everything else (`types`, `cache`, `installer`, `installs`, `registry`,
//! `refresh`, `user_registry`, …) is reached through the marketplace
//! crate's `prelude` and re-exported here only when a call site reads
//! more naturally as `crate::marketplace::…`.

pub mod scheduler;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use arbor_plugin_marketplace::prelude::{MarketplaceHost, MarketplaceRegistry};
use arbor_plugin_types::prelude::Manifest;

// ---------------------------------------------------------------------------
// MarketplaceHost impl
// ---------------------------------------------------------------------------

/// Wires the marketplace crate's `MarketplaceHost` trait to the live
/// `arbor-plugin-core` runtime so the catalog's local-merge step can see
/// dev / hand-copied plugins (and their enable flags) without the
/// marketplace crate having to depend on `arbor-plugin-core`.
pub struct TauriMarketplaceHost;

impl MarketplaceHost for TauriMarketplaceHost {
    fn discover_plugins(&self) -> Vec<Manifest> {
        arbor_plugin_core::prelude::discover_plugins().unwrap_or_default()
    }

    fn plugin_states(&self) -> HashMap<String, bool> {
        arbor_plugin_core::prelude::load_plugin_states()
    }

    fn dev_plugin_dir(&self) -> PathBuf {
        arbor_plugin_core::prelude::plugin_dir()
    }
}

/// Build the in-memory registry with the Tauri-side host wired in. Called
/// once at `AppState` construction.
pub fn build_registry() -> MarketplaceRegistry {
    MarketplaceRegistry::new(Arc::new(TauriMarketplaceHost))
}
