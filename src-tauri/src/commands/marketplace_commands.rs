//! IPC bridge for the plugin & theme marketplace.
//!
//! Phase 3 wires the real zipball installer (`marketplace::installer`) and
//! enable-toggle plumbing through to `PluginHost`. The marketplace catalog
//! is a pure remote view; `installed` flags come from
//! `marketplace_installed.json` (see `marketplace::installs`).
//!
//! Dev / hand-copied plugins are NOT reconciled with this catalog — that
//! separation is by design.

use std::sync::MutexGuard;

use serde::Deserialize;
use tauri::{Emitter, State};

use crate::error::{AppError, Result};
use crate::marketplace::{
    self,
    types::{
        MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme,
        RegistryEntry,
    },
    user_registry::UserSource,
    MarketplaceRegistry,
};
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

/// Synchronous slice rendered on modal open: only the entries actually
/// installed *through the marketplace* (i.e. tracked in
/// `marketplace_installed.json`). Dev / hand-copied plugins do NOT appear
/// here.
#[tauri::command]
pub fn marketplace_list_installed(
    state: State<'_, AppState>,
) -> Result<MarketplaceCatalog> {
    Ok(lock(&state)?.installed_only())
}

/// Full catalog. Uses the 1h disk cache when fresh; otherwise refreshes
/// from the network and writes through to the cache.
#[tauri::command]
pub async fn marketplace_fetch_registry(
    state: State<'_, AppState>,
) -> Result<MarketplaceCatalog> {
    let needs_refresh = !lock(&state)?.has_fresh_cache();
    if needs_refresh {
        marketplace::refresh_community(&state.marketplace).await?;
    }
    Ok(lock(&state)?.catalog())
}

/// Bypass the cache, force a fresh fetch. Wired to the modal's Refresh
/// button.
#[tauri::command]
pub async fn marketplace_refresh_registry(
    state: State<'_, AppState>,
) -> Result<MarketplaceCatalog> {
    marketplace::cache::invalidate();
    marketplace::refresh_community(&state.marketplace).await?;
    Ok(lock(&state)?.catalog())
}

/// Returns the set of plugin names installed via the marketplace. The
/// Plugin Manager uses this to decorate matching rows with a "Marketplace"
/// badge so dev plugins are visually distinguishable.
#[tauri::command]
pub fn marketplace_installed_plugin_names() -> Result<Vec<String>> {
    let installs = marketplace::installs::load();
    Ok(installs.plugins.keys().cloned().collect())
}

// ---------------------------------------------------------------------------
// Auto-refresh interval
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn marketplace_get_refresh_hours(state: State<'_, AppState>) -> Result<Option<u32>> {
    let cfg = state.lock_config()?;
    Ok(cfg.marketplace.refresh_hours)
}

/// Set the auto-refresh interval in hours. `None` or `Some(0)` disables
/// the scheduler. The change takes effect on the next poll cycle.
#[tauri::command]
pub fn marketplace_set_refresh_hours(
    state: State<'_, AppState>,
    hours: Option<u32>,
) -> Result<()> {
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.refresh_hours = match hours {
            Some(0) => None,
            other   => other,
        };
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace refresh hours: {e}")))?;
    Ok(())
}

#[tauri::command]
pub fn marketplace_get_poll_minutes(state: State<'_, AppState>) -> Result<u32> {
    let cfg = state.lock_config()?;
    Ok(cfg.marketplace.poll_minutes)
}

/// How often the background scheduler wakes up to check whether a refresh
/// is due. Clamped to [1, 60] in the scheduler regardless of what's
/// persisted — values outside the range fall back to the default.
#[tauri::command]
pub fn marketplace_set_poll_minutes(
    state:   State<'_, AppState>,
    minutes: u32,
) -> Result<()> {
    let snapshot = {
        let mut cfg = state.lock_config()?;
        cfg.marketplace.poll_minutes = minutes.clamp(1, 60);
        cfg.clone()
    };
    crate::config::app_config::save(&snapshot)
        .map_err(|e| AppError::Other(format!("could not persist marketplace poll minutes: {e}")))?;
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
    // hitting the network.
    let plugin = {
        let reg = lock(&state)?;
        reg.find_plugin(&name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not in catalog")))?
    };

    let installed = marketplace::installer::install_plugin(&plugin).await?;
    marketplace::installs::record_plugin(installed);

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
        marketplace::installs::set_plugin_enabled(other, false);
    }

    marketplace::installer::uninstall_plugin(&name)?;
    marketplace::installs::forget_plugin(&name);

    // Wipe the host's enable-state entry too — keeps the ledger clean.
    let mut states = crate::plugin::runtime::manifest::load_plugin_states();
    states.remove(&name);
    crate::plugin::runtime::manifest::save_plugin_states(&states);

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
    state:   State<'_, AppState>,
    name:    String,
    enabled: bool,
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
    marketplace::installs::set_plugin_enabled(&name, enabled);
    for other in &cascaded {
        if other != &name {
            marketplace::installs::set_plugin_enabled(other, enabled);
        }
    }

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
    let installed = marketplace::installer::install_theme(&theme).await?;
    marketplace::installs::record_theme(installed);

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
    marketplace::installer::uninstall_theme(&id)?;
    marketplace::installs::forget_theme(&id);
    let _ = app_handle.emit("arbor://themes-changed", ());

    Ok(lock(&state)?
        .catalog()
        .themes
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| stub_theme(&id)))
}

// ---------------------------------------------------------------------------
// Custom source (Phase 4 — async resolve + persist)
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
    marketplace::add_custom_source(&state.marketplace, source).await
}

#[derive(Debug, Deserialize)]
pub struct RemoveCustomSourceArgs {
    pub repo:    String,
    pub subpath: Option<String>,
}

/// Forget a user-added source. Composite key `(repo, subpath)` — the same
/// repo can host multiple distinct entries pointing at different subpaths.
/// Installed plugins from this source are NOT auto-uninstalled — the
/// install registry remains the source-of-truth for installed state.
#[tauri::command]
pub fn marketplace_remove_custom_source(
    state: State<'_, AppState>,
    args:  RemoveCustomSourceArgs,
) -> Result<bool> {
    marketplace::remove_custom_source(&state.marketplace, &args.repo, args.subpath.as_deref())
}

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
        preview:     marketplace::types::MarketplaceThemePreview {
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
        },
    }
}
