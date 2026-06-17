//! `marketplace` domain — leaf-clean reads + the synchronous custom-source
//! removal, routed through the platform backend.
//!
//! Only the side-effect-free, AppHandle-free, network-free slice of the
//! marketplace command surface lives here. Everything that downloads /
//! installs / updates a plugin or theme (network + job + `arbor://*` emit),
//! re-arms the refresh scheduler, or fetches the catalog over HTTP stays a
//! keep-shell Tauri command in `commands/marketplace_commands.rs`.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline, now
//! self-registered under `program = "platform"`. The registry-mutex helper is
//! re-expressed against `&AppState` (the handler context) instead of the
//! `State<'_, AppState>` Tauri extractor.

use std::sync::MutexGuard;

use serde::Deserialize;

use arbor_plugin_marketplace::prelude as mk;
use mk::{MarketplaceCatalog, MarketplaceRegistry};

use crate::error::{AppError, Result};
use crate::ipc::platform;
use crate::AppState;

fn lock(state: &AppState) -> Result<MutexGuard<'_, MarketplaceRegistry>> {
    state.marketplace.lock().map_err(|e| {
        tracing::error!("marketplace registry mutex poisoned: {e}");
        AppError::MutexPoisoned("marketplace".into())
    })
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// Synchronous slice rendered on modal open: only the entries actually
/// installed *through the marketplace* (i.e. tracked in
/// `marketplace_installed.json`). Dev / hand-copied plugins do NOT appear
/// here.
#[platform::handler(program = "platform")]
fn marketplace_list_installed(state: &AppState) -> Result<MarketplaceCatalog> {
    Ok(lock(state)?.installed_only())
}

/// Returns the set of plugin names installed via the marketplace. The
/// Plugin Manager uses this to decorate matching rows with a "Marketplace"
/// badge so dev plugins are visually distinguishable.
#[platform::handler(program = "platform")]
fn marketplace_installed_plugin_names(_state: &AppState) -> Result<Vec<String>> {
    Ok(mk::load_installs().plugins.keys().cloned().collect())
}

// ---------------------------------------------------------------------------
// Auto-refresh interval (read-only; the setters re-arm the scheduler and stay
// keep-shell)
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn marketplace_get_refresh_hours(state: &AppState) -> Result<Option<u32>> {
    let cfg = state.lock_config()?;
    Ok(cfg.marketplace.refresh_hours)
}

#[platform::handler(program = "platform")]
fn marketplace_get_poll_minutes(state: &AppState) -> Result<u32> {
    let cfg = state.lock_config()?;
    Ok(cfg.marketplace.poll_minutes)
}

// ---------------------------------------------------------------------------
// Custom source — synchronous removal
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RemoveCustomSourceArgs {
    pub repo:    String,
    pub subpath: Option<String>,
}

/// Forget a user-added source. Composite key `(repo, subpath)` — the same
/// repo can host multiple distinct entries pointing at different subpaths.
/// Installed plugins from this source are NOT auto-uninstalled — the
/// install registry remains the source-of-truth for installed state.
#[platform::handler(program = "platform")]
fn marketplace_remove_custom_source(state: &AppState, args: RemoveCustomSourceArgs) -> Result<bool> {
    Ok(mk::remove_custom_source(&state.marketplace, &args.repo, args.subpath.as_deref())?)
}
