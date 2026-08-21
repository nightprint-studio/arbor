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

    fn forget_plugin_credentials(&self, plugin: &str) {
        // Best-effort: a keychain that refuses is not a reason to leave the plugin
        // half-uninstalled. The user asked for it gone, and the files going is the part
        // they can see.
        if let Err(e) = crate::auth::credential_store::forget_plugin(plugin) {
            tracing::warn!("uninstall '{plugin}': could not clear its credentials: {e}");
        }
    }
}

/// Build the in-memory registry with the Tauri-side host wired in. Called
/// once at `AppState` construction.
pub fn build_registry() -> MarketplaceRegistry {
    MarketplaceRegistry::new(Arc::new(TauriMarketplaceHost))
}

/// Give every package the marketplace installed a product, once.
///
/// ## The distinction this draws
///
/// A package's enable-state has three readings, and only two of them were ever recorded:
/// "on", "off", and **nothing said**. Nothing-said has to mean *load it* — that is how a
/// folder dropped into `plugins/` runs, and how every plugin in this repo's dev tree runs.
///
/// But it was also the answer for packages the user **deliberately installed**, back when
/// installing was not a per-product act. So Bennu's Plugin Manager listed Corvus's git
/// plugins: nobody had said they did not belong there, because until now there was no
/// "there" to say it about.
///
/// The marketplace ledger is exactly the line between the two. Its own doc says so — it
/// records what was *downloaded through it*, and a hand-copied or dev folder never appears.
/// So: a package in the ledger with no product yet belongs to **Corvus**, which is not a guess
/// — the Marketplace is reachable from Corvus's title bar and nowhere else, so it is the only
/// place any of them could have been installed from.
///
/// ## Idempotent by construction
///
/// It skips any package that already has a per-product entry, so a second boot does nothing
/// and — more importantly — enabling one of them for Bennu afterwards is not undone the next
/// time the app starts.
pub fn scope_existing_installs_to_their_product() {
    use arbor_plugin_core::prelude as core;

    let ledger = arbor_plugin_marketplace::prelude::load_installs();
    if ledger.plugins.is_empty() {
        return;
    }
    let mut states = core::load_states();
    let mut changed = false;

    for name in ledger.plugins.keys() {
        if states.products.values().any(|m| m.contains_key(name)) {
            continue;
        }
        // `scope_to`, not `install_for`: a package the user switched OFF must not come back on
        // because it was being given a product.
        states.scope_to(arbor_plugin_types::prelude::PRODUCT_CORVUS, name);
        changed = true;
    }

    if changed {
        tracing::info!(
            "plugins: gave {} previously-installed package(s) their product",
            ledger.plugins.len()
        );
        core::save_states(&states);
    }
}
