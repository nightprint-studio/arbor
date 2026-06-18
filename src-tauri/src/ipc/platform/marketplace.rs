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
use mk::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceRegistry, MarketplaceSource, MarketplaceTheme,
    MarketplaceThemePreview, RegistryEntry, UserSource,
};

use crate::error::{AppError, Result};
use crate::ipc::platform;
use crate::marketplace;
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

// ===========================================================================
// emit/seam pass — network installs + catalog fetches moved off `AppHandle`.
//
// These download/install/uninstall over HTTP (async where networked) and used
// to take an `AppHandle` solely to emit `arbor://plugins-reloaded` /
// `arbor://themes-changed`; that emit now goes through the backend event sink
// (`state.emit`). The two interval **setters** (`marketplace_set_refresh_hours`
// / `marketplace_set_poll_minutes`) stay inline in `commands` — they re-arm the
// running refresh scheduler, which needs the `AppHandle`.
// ===========================================================================

/// Reload the plugin host after a marketplace install/uninstall so the new /
/// removed folder is picked up, and broadcast `arbor://plugins-reloaded`.
/// Unlike the Plugin Manager's reload it does NOT re-fire repo lifecycle hooks
/// (an install shouldn't replay `on_repo_open` for every tab).
fn reload_host(state: &AppState) -> Result<()> {
    // Cancel any background job tied to a plugin that's about to be reloaded.
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.cancel_by_plugin(None);
    }
    {
        let mut host = state.lock_plugin_host()?;
        host.reload()?;
        host.start_all_schedulers();
    }
    state.emit("arbor://plugins-reloaded", ());
    Ok(())
}

// ── Catalog fetch (async HTTP, cache-backed) ─────────────────────────────────

/// Full catalog. Uses the 1h disk cache when fresh; otherwise refreshes from
/// the network and writes through to the cache.
#[platform::handler(program = "platform")]
async fn marketplace_fetch_registry(state: &AppState) -> Result<MarketplaceCatalog> {
    let needs_refresh = !lock(state)?.has_fresh_cache();
    if needs_refresh {
        mk::refresh_community(&state.marketplace).await?;
    }
    Ok(lock(state)?.catalog())
}

/// Bypass the cache, force a fresh fetch (the modal's Refresh button).
#[platform::handler(program = "platform")]
async fn marketplace_refresh_registry(state: &AppState) -> Result<MarketplaceCatalog> {
    mk::invalidate_cache();
    mk::refresh_community(&state.marketplace).await?;
    Ok(lock(state)?.catalog())
}

// ── Plugin install / uninstall / toggle ──────────────────────────────────────

/// Download a plugin's zipball, extract it, record the install, and reload the
/// plugin host so it's discovered (still disabled — the user opts in).
#[platform::handler(program = "platform")]
async fn marketplace_install_plugin(state: &AppState, name: String) -> Result<MarketplacePlugin> {
    // Resolve the catalog entry — clone out so we drop the mutex before hitting
    // the network. The host reference comes from the same dev-plugin dir the
    // rest of the marketplace surface uses.
    let (plugin, host) = {
        let reg = lock(state)?;
        let plugin = reg.find_plugin(&name)
            .ok_or_else(|| AppError::Other(format!("plugin '{name}' not in catalog")))?;
        (plugin, marketplace::TauriMarketplaceHost)
    };

    let installed = mk::install_plugin(&host, &plugin).await?;
    mk::record_plugin(installed);

    reload_host(state)?;

    // Re-resolve from the catalog so installed/enabled are populated.
    lock(state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| AppError::Other(format!("installed '{name}' but vanished from catalog")))
}

#[platform::handler(program = "platform")]
fn marketplace_uninstall_plugin(state: &AppState, name: String) -> Result<MarketplacePlugin> {
    // Cascade-disable required dependents BEFORE the folder is removed so they
    // don't keep running against a vanished service/hook target, and flip the
    // marketplace ledger so the modal doesn't show them as "enabled".
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

    reload_host(state)?;

    Ok(lock(state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        // A custom-only entry may no longer be in the catalog after uninstall —
        // return a stub so the FE can paint "available again".
        .unwrap_or_else(|| stub_plugin(&name)))
}

#[platform::handler(program = "platform")]
fn marketplace_set_plugin_enabled(
    state: &AppState,
    name: String,
    enabled: bool,
) -> Result<MarketplacePlugin> {
    // Mirror the change through the host so the live VM picks it up. Both sides
    // cascade; capture the list so the ledger stays in sync for everything
    // touched, not just the user-clicked plugin.
    let cascaded: Vec<String> = {
        let mut host = state.lock_plugin_host()?;
        if enabled { host.enable_plugin(&name)? }
        else       { host.disable_plugin(&name)? }
    };

    mk::set_plugin_enabled(&name, enabled);
    for other in &cascaded {
        if other != &name {
            mk::set_plugin_enabled(other, enabled);
        }
    }

    // Toggle skips the host reload, so it needs its own explicit emit to keep a
    // background Plugin Manager in sync.
    state.emit("arbor://plugins-reloaded", ());

    lock(state)?
        .catalog()
        .plugins
        .into_iter()
        .find(|p| p.name == name)
        .ok_or_else(|| AppError::Other(format!("plugin '{name}' not in catalog")))
}

// ── Theme install / uninstall ────────────────────────────────────────────────

#[platform::handler(program = "platform")]
async fn marketplace_install_theme(state: &AppState, id: String) -> Result<MarketplaceTheme> {
    let theme = {
        let reg = lock(state)?;
        reg.find_theme(&id)
            .ok_or_else(|| AppError::Other(format!("theme '{id}' not in catalog")))?
    };
    let installed = mk::install_theme(&theme).await?;
    mk::record_theme(installed);

    // Tell the frontend so the Settings → Appearance picker picks it up.
    state.emit("arbor://themes-changed", ());

    lock(state)?
        .catalog()
        .themes
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| AppError::Other(format!("installed theme '{id}' but vanished from catalog")))
}

#[platform::handler(program = "platform")]
fn marketplace_uninstall_theme(state: &AppState, id: String) -> Result<MarketplaceTheme> {
    mk::uninstall_theme(&id)?;
    mk::forget_theme(&id);
    state.emit("arbor://themes-changed", ());

    Ok(lock(state)?
        .catalog()
        .themes
        .into_iter()
        .find(|t| t.id == id)
        .unwrap_or_else(|| stub_theme(&id)))
}

// ── Custom source — async resolve + persist ──────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AddCustomSourceArgs {
    pub repo:        String,
    #[serde(rename = "ref")]
    pub r#ref:       Option<String>,
    pub subpath:     Option<String>,
    pub pinned_sha:  Option<String>,
    pub description: Option<String>,
}

/// Resolve a user-supplied GitHub URL via the 3-mode resolver, persist the
/// pointer, cache the resolved metadata, and return the resolved plugin(s).
#[platform::handler(program = "platform")]
async fn marketplace_add_custom_source(
    state: &AppState,
    args: AddCustomSourceArgs,
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

// ---------------------------------------------------------------------------
// Stubs for entries that left the catalog after uninstall.
// ---------------------------------------------------------------------------

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
