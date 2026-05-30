//! Async helpers wrapping `fetch_catalog` + custom-source resolution.
//!
//! Used by the command layer to refresh the community catalog (writing
//! through to the on-disk cache as a side effect), re-resolve every
//! user-added custom source, and add or remove individual custom sources.

use std::sync::Mutex;

use crate::custom::{resolve_custom_source, CustomSourceResolution};
use crate::error::{MarketplaceError, Result};
use crate::github_api::client;
use crate::index::fetch_catalog;
use crate::registry::MarketplaceRegistry;
use crate::types::{MarketplacePlugin, MarketplaceSource};
use crate::user_registry::{self, UserSource};

/// Refresh the community catalog from the network and stash it (also
/// writes through to the disk cache). Also re-resolves every user-added
/// custom source so a single Refresh keeps both lists in sync.
pub async fn refresh_community(reg_mutex: &Mutex<MarketplaceRegistry>) -> Result<()> {
    let repo_url = {
        let reg = reg_mutex.lock().map_err(|_| poisoned())?;
        reg.community_repo().to_string()
    };
    let http = client()?;
    let catalog = fetch_catalog(&http, &repo_url, MarketplaceSource::Community).await?;
    {
        let mut reg = reg_mutex.lock().map_err(|_| poisoned())?;
        reg.set_community(catalog);
    }
    // Best-effort custom refresh — failures here are logged per source but
    // don't fail the community refresh as a whole.
    if let Err(e) = refresh_custom(reg_mutex, &http).await {
        tracing::warn!("custom-source refresh failed: {e}");
    }
    Ok(())
}

/// Re-resolve every source in `user_registry.toml` and replace the
/// in-memory custom list. Per-source failures are dropped (logged) so a
/// single broken pointer doesn't blank the rest.
pub async fn refresh_custom(
    reg_mutex: &Mutex<MarketplaceRegistry>,
    http:      &reqwest::Client,
) -> Result<()> {
    let sources = user_registry::load().sources;
    let mut resolved: Vec<MarketplacePlugin> = Vec::new();
    for src in sources {
        match resolve_custom_source(http, &src.repo, src.r#ref.as_deref(), src.subpath.as_deref()).await {
            Ok(CustomSourceResolution::Single(p))      => resolved.push(p),
            Ok(CustomSourceResolution::Multi(plugins)) => resolved.extend(plugins),
            Err(e) => tracing::warn!(
                "custom source {} (subpath={:?}) failed to resolve: {e}",
                src.repo, src.subpath
            ),
        }
    }
    let mut reg = reg_mutex.lock().map_err(|_| poisoned())?;
    reg.set_custom(resolved);
    Ok(())
}

/// Resolve + persist a brand-new custom source. Returns the plugins it
/// resolved to so the FE can paint them immediately.
pub async fn add_custom_source(
    reg_mutex: &Mutex<MarketplaceRegistry>,
    source:    UserSource,
) -> Result<Vec<MarketplacePlugin>> {
    let http = client()?;
    let res  = resolve_custom_source(
        &http,
        &source.repo,
        source.r#ref.as_deref(),
        source.subpath.as_deref(),
    ).await?;
    let plugins: Vec<MarketplacePlugin> = match res {
        CustomSourceResolution::Single(p) => vec![p],
        CustomSourceResolution::Multi(v)  => v,
    };
    // Persist the pointer first — if the resolver re-runs on Refresh
    // we'll re-fetch from the network rather than relying on the cache.
    user_registry::add(source);
    let mut reg = reg_mutex.lock().map_err(|_| poisoned())?;
    reg.merge_custom_plugins(plugins.clone());
    Ok(plugins)
}

/// Remove a custom source. The composite key is `(repo, subpath)` so the
/// same repo can host multiple distinct sources.
pub fn remove_custom_source(
    reg_mutex: &Mutex<MarketplaceRegistry>,
    repo:      &str,
    subpath:   Option<&str>,
) -> Result<bool> {
    let removed = user_registry::remove(repo, subpath);
    if removed {
        let mut reg = reg_mutex.lock().map_err(|_| poisoned())?;
        reg.drop_custom_by_pointer(repo, subpath);
    }
    Ok(removed)
}

fn poisoned() -> MarketplaceError {
    MarketplaceError::Other("marketplace registry mutex poisoned".into())
}
