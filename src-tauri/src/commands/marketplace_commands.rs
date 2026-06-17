//! IPC bridge for the plugin & theme marketplace.
//!
//! Catalog, cache, installer, install ledger, custom-source resolver, and
//! refresh helpers all live in `arbor-plugin-marketplace`. This module is
//! a thin Tauri-side adapter — it locks the in-memory registry, maps
//! `MarketplaceError` to `AppError` via `?`, mirrors the install /
//! uninstall / enable transitions into the host's plugin state, and
//! emits the matching `arbor://*` notifications.
//!
//! Dev / hand-copied plugins are NOT reconciled with this catalog — the
//! `Local` rows surface through the same `MarketplaceRegistry::catalog`
//! call but live in their own pool (see the crate's README).

use std::sync::MutexGuard;

use serde::Deserialize;
use tauri::{Emitter, State};

use arbor_plugin_marketplace::prelude as mk;
use mk::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceRegistry, MarketplaceSource,
    MarketplaceTheme, MarketplaceThemePreview, RegistryEntry, UserSource,
};

use crate::error::{AppError, Result};
use crate::marketplace;
use crate::AppState;

fn lock<'a>(state: &'a State<'a, AppState>) -> Result<MutexGuard<'a, MarketplaceRegistry>> {
    state.marketplace.lock().map_err(|e| {
        tracing::error!("marketplace registry mutex poisoned: {e}");
        AppError::MutexPoisoned("marketplace".into())
    })
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------
//
// `marketplace_list_installed` and `marketplace_installed_plugin_names` moved
// to the platform backend (`ipc/platform/marketplace.rs`) — they're leaf-clean
// synchronous reads. The cache-backed registry fetches below stay here:
// they're genuinely async (HTTP) and write through the disk cache.

/// Full catalog. Uses the 1h disk cache when fresh; otherwise refreshes
/// from the network and writes through to the cache.
#[tauri::command]
pub async fn marketplace_fetch_registry(
    state: State<'_, AppState>,
) -> Result<MarketplaceCatalog> {
    let needs_refresh = !lock(&state)?.has_fresh_cache();
    if needs_refresh {
        mk::refresh_community(&state.marketplace).await?;
    }
    Ok(lock(&state)?.catalog())
}

/// Bypass the cache, force a fresh fetch. Wired to the modal's Refresh
/// button.
#[tauri::command]
pub async fn marketplace_refresh_registry(
    state: State<'_, AppState>,
) -> Result<MarketplaceCatalog> {
    mk::invalidate_cache();
    mk::refresh_community(&state.marketplace).await?;
    Ok(lock(&state)?.catalog())
}

// ---------------------------------------------------------------------------
// Auto-refresh interval
// ---------------------------------------------------------------------------
//
// The read-only getters (`marketplace_get_refresh_hours` /
// `marketplace_get_poll_minutes`) moved to the platform backend. The setters
// stay here: they take an `AppHandle` and re-arm the running scheduler.

/// Set the auto-refresh interval in hours. `None` or `Some(0)` disables
/// the scheduler. The change takes effect on the next poll cycle.
#[tauri::command]
pub fn marketplace_set_refresh_hours(
    app:   tauri::AppHandle,
    state: State<'_, AppState>,
    hours: Option<u32>,
) -> Result<()> {
    let normalized = match hours {
        Some(0) => None,
        other   => other,
    };
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.refresh_hours = normalized;
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace refresh hours: {e}")))?;
    // Park / re-arm the running schedule without restarting it.
    marketplace::scheduler::apply_refresh_hours(&app, normalized);
    Ok(())
}

/// How often the background scheduler wakes up to check whether a refresh
/// is due. Clamped to [1, 60] in the scheduler regardless of what's
/// persisted — values outside the range fall back to the default.
#[tauri::command]
pub fn marketplace_set_poll_minutes(
    app:     tauri::AppHandle,
    state:   State<'_, AppState>,
    minutes: u32,
) -> Result<()> {
    let clamped = minutes.clamp(1, 60);
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.poll_minutes = clamped;
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace poll minutes: {e}")))?;
    // Swap the running schedule's cadence on the fly.
    marketplace::scheduler::apply_poll_minutes(&app, clamped);
    Ok(())
}

// ---------------------------------------------------------------------------
// Plugin install / uninstall
// ---------------------------------------------------------------------------

/// Download a plugin's zipball from GitHub, extract it to
/// `marketplace_plugins/{name}/`, record the install, and reload the
/// plugin host so the new plugin is discovered (still disabled — the user
/// opts in from the detail pane).
#[tauri::command]
pub async fn marketplace_install_plugin(
    app_handle: tauri::AppHandle,
    state:      State<'_, AppState>,
    name:       String,
) -> Result<MarketplacePlugin> {
    // Resolve the catalog entry — clone out so we drop the mutex before
    // hitting the network. The host reference is taken from the registry
    // so the installer sees the same dev-plugin dir the rest of the
    // marketplace surface uses.
    let (plugin, host) = {
        let reg = lock(&state)?;
        let plugin = reg.find_plugin(&name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not in catalog")))?;
        (plugin, marketplace::TauriMarketplaceHost)
    };

    let installed = mk::install_plugin(&host, &plugin).await?;
    mk::record_plugin(installed);

    // Tell the host to re-scan so the new folder is picked up.
    reload_plugin_host(&app_handle, &state)?;

    // Re-resolve from the catalog so installed/enabled are populated.
    lock(&state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| AppError::Other(format!("installed '{name}' but vanished from catalog")))
}

#[tauri::command]
pub fn marketplace_uninstall_plugin(
    app_handle: tauri::AppHandle,
    state:      State<'_, AppState>,
    name:       String,
) -> Result<MarketplacePlugin> {
    // Cascade-disable required dependents BEFORE the folder is removed —
    // mirrors the Plugin Manager's uninstall path. Without this, dependents
    // stay running with a vanished service / hook target until the next
    // reload (where they'd land in load_failures). The cascade also flips
    // the marketplace ledger so the modal doesn't show them as "enabled"
    // immediately after the operation.
    let cascaded: Vec<String> = {
        let mut host = state.lock_plugin_host()?;
        host.disable_required_dependents(&name)
    };
    for other in &cascaded {
        mk::set_plugin_enabled(other, false);
    }

    mk::uninstall_plugin(&name)?;
    mk::forget_plugin(&name);

    // Wipe the host's enable-state entry too — keeps the ledger clean.
    let mut states = arbor_plugin_core::prelude::load_plugin_states();
    states.remove(&name);
    arbor_plugin_core::prelude::save_plugin_states(&states);

    reload_plugin_host(&app_handle, &state)?;

    Ok(lock(&state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        // Uninstalled entries still appear in the catalog as "available
        // again" — but if the user uninstalled a custom-only entry the
        // catalog may no longer carry it. In that case return a stub.
        .unwrap_or_else(|| stub_plugin(&name)))
}

#[tauri::command]
pub fn marketplace_set_plugin_enabled(
    app_handle: tauri::AppHandle,
    state:      State<'_, AppState>,
    name:       String,
    enabled:    bool,
) -> Result<MarketplacePlugin> {
    // Mirror the change through the host so the live VM picks it up. Both
    // sides cascade — disabling a plugin disables every transitively-required
    // dependent, enabling one enables its required deps. We capture the
    // returned cascade list so the marketplace ledger stays in sync for all
    // plugins touched, not just the user-clicked one.
    let cascaded: Vec<String> = {
        let mut host = state.lock_plugin_host()?;
        if enabled { host.enable_plugin(&name)? }
        else       { host.disable_plugin(&name)? }
    };

    // Update the marketplace ledger for every plugin actually flipped (so
    // the modal reflects state across restarts even without a host re-scan).
    // The cascade list excludes the target when it was already in the desired
    // state — write it explicitly to handle that corner case.
    mk::set_plugin_enabled(&name, enabled);
    for other in &cascaded {
        if other != &name {
            mk::set_plugin_enabled(other, enabled);
        }
    }

    // Notify every listener (Plugin Manager, contribution store, sidebar
    // panels…) that plugin state changed. Without this, a toggle from the
    // marketplace silently desyncs the Plugin Manager if it's open in the
    // background — the user would have to close + reopen the panel to see
    // the new state. Install / uninstall already emit via `reload_plugin_host`;
    // toggle skips the host reload, so it needs its own explicit emit.
    let _ = app_handle.emit("arbor://plugins-reloaded", ());

    lock(&state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| AppError::Other(format!("plugin '{name}' not in catalog")))
}

// ---------------------------------------------------------------------------
// Theme install / uninstall
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn marketplace_install_theme(
    app_handle: tauri::AppHandle,
    state:      State<'_, AppState>,
    id:         String,
) -> Result<MarketplaceTheme> {
    let theme = {
        let reg = lock(&state)?;
        reg.find_theme(&id)
            .ok_or_else(|| AppError::Other(format!("theme '{id}' not in catalog")))?
    };
    let installed = mk::install_theme(&theme).await?;
    mk::record_theme(installed);

    // Tell the frontend so the Settings → Appearance picker picks it up.
    let _ = app_handle.emit("arbor://themes-changed", ());

    lock(&state)?
        .catalog()
        .themes
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::Other(format!("installed theme '{id}' but vanished from catalog")))
}

#[tauri::command]
pub fn marketplace_uninstall_theme(
    app_handle: tauri::AppHandle,
    state:      State<'_, AppState>,
    id:         String,
) -> Result<MarketplaceTheme> {
    mk::uninstall_theme(&id)?;
    mk::forget_theme(&id);
    let _ = app_handle.emit("arbor://themes-changed", ());

    Ok(lock(&state)?
        .catalog()
        .themes
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| stub_theme(&id)))
}

// ---------------------------------------------------------------------------
// Custom source — async resolve + persist
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AddCustomSourceArgs {
    pub repo:        String,
    #[serde(rename = "ref")]
    pub r#ref:       Option<String>,
    pub subpath:     Option<String>,
    pub pinned_sha:  Option<String>,
    pub description: Option<String>,
}

/// Resolve a user-supplied GitHub URL via the 3-mode resolver
/// (`subpath` → `plugin.toml@root` → `index.json@root`), persist the
/// pointer to `user_registry.toml`, cache the resolved metadata, and
/// return the resolved plugin(s) so the FE can paint them immediately.
#[tauri::command]
pub async fn marketplace_add_custom_source(
    state: State<'_, AppState>,
    args:  AddCustomSourceArgs,
) -> Result<Vec<MarketplacePlugin>> {
    let source = UserSource {
        repo:        args.repo,
        r#ref:       args.r#ref,
        subpath:     args.subpath,
        pinned_sha:  args.pinned_sha,
        description: args.description,
    };
    Ok(mk::add_custom_source(&state.marketplace, source).await?)
}

// `marketplace_remove_custom_source` moved to the platform backend
// (`ipc/platform/marketplace.rs`) — synchronous, state-only, no emit.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn reload_plugin_host(app_handle: &tauri::AppHandle, state: &State<'_, AppState>) -> Result<()> {
    // Cancel any background job tied to a plugin that's about to be reloaded.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(None);
    }
    {
        let mut host = state.lock_plugin_host()?;
        host.reload()?;
        host.start_all_schedulers();
    }
    let _ = app_handle.emit("arbor://plugins-reloaded", ());
    Ok(())
}

fn stub_plugin(name: &str) -> MarketplacePlugin {
    MarketplacePlugin {
        name:        name.into(),
        version:     "?".into(),
        description: String::new(),
        author:      String::new(),
        category:    None,
        tags:        None,
        repository:  None,
        homepage:    None,
        min_arbor_version: None,
        icon:        None,
        screenshots: None,
        permissions: None,
        source:      MarketplaceSource::Local,
        installed:   false,
        enabled:     None,
        entry: RegistryEntry {
            repo: String::new(), r#ref: None, subpath: None,
            source: MarketplaceSource::Local, pinned_sha: None,
            external: false,
        },
        experimental: None,
        doc:         None,
        update_available:  None,
        installed_version: None,
        dependencies: Vec::new(),
    }
}

fn stub_theme(id: &str) -> MarketplaceTheme {
    MarketplaceTheme {
        id:          id.into(),
        name:        id.into(),
        description: String::new(),
        author:      None,
        tags:        None,
        preview:     MarketplaceThemePreview {
            bg: "#000".into(), fg: "#fff".into(),
            accent: "#000".into(), success: "#000".into(),
            warning: "#000".into(), error: "#000".into(),
        },
        variant:     None,
        source:      MarketplaceSource::Local,
        installed:   false,
        entry: RegistryEntry {
            repo: String::new(), r#ref: None, subpath: None,
            source: MarketplaceSource::Local, pinned_sha: None,
            external: false,
        },
    }
}
