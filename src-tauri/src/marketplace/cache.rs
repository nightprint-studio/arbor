//! Disk-backed cache for the fetched marketplace catalog.
//!
//! Lives at `~/.config/arbor/marketplace_cache.json` (or
//! `marketplace_cache-dev.json` in debug builds, so dev sessions don't
//! poison the prod cache of a side-by-side Arbor install).
//!
//! TTL is fixed at 1h — the modal carries a "Refresh" button for the rare
//! moments the user wants newer data immediately.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::types::{MarketplaceCatalog, MarketplacePlugin};

/// Time-to-live for a cached catalog snapshot.
pub const TTL_SECS: u64 = 3600;

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheFile {
    pub fetched_at: u64,
    /// Source repo URL — guards against picking up the prod cache after
    /// switching the curated repo to a fork.
    pub repo:       String,
    pub catalog:    MarketplaceCatalog,
}

pub fn path() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "marketplace_cache-dev.json"
    } else {
        "marketplace_cache.json"
    };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(filename)
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Read the cache file regardless of age — useful as a cold-start fallback
/// so the modal has something to show before the first refresh resolves.
pub fn load_any() -> Option<CacheFile> {
    let s = std::fs::read_to_string(path()).ok()?;
    serde_json::from_str(&s).ok()
}

/// Return the cached catalog if it was fetched from `repo` and is younger
/// than `TTL_SECS`. Otherwise `None` and the caller should refresh.
pub fn load_if_fresh(repo: &str) -> Option<MarketplaceCatalog> {
    let f = load_any()?;
    if f.repo != repo { return None; }
    let age = now_secs().saturating_sub(f.fetched_at);
    if age > TTL_SECS {
        tracing::debug!("marketplace cache stale ({age}s > {TTL_SECS}s)");
        return None;
    }
    Some(f.catalog)
}

pub fn save(repo: &str, catalog: &MarketplaceCatalog) {
    let p = path();
    if let Some(parent) = p.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("marketplace cache: create_dir_all failed: {e}");
            return;
        }
    }
    let file = CacheFile {
        fetched_at: now_secs(),
        repo:       repo.to_string(),
        catalog:    catalog.clone(),
    };
    match serde_json::to_string_pretty(&file) {
        Ok(s) => {
            if let Err(e) = std::fs::write(&p, s) {
                tracing::warn!("marketplace cache write {p:?} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("marketplace cache serialise failed: {e}"),
    }
}

pub fn invalidate() {
    let _ = std::fs::remove_file(path());
}

// ---------------------------------------------------------------------------
// Custom-source resolved cache
// ---------------------------------------------------------------------------
//
// Resolved metadata for user-added custom sources (Phase 4). Pointers live
// in `user_registry.toml`; this file caches the result of the last
// successful network resolve so a cold offline boot still has something
// to paint. No TTL — the cache is refreshed every time the user opens the
// modal (alongside the community catalog refresh).

fn custom_cache_path() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "marketplace_custom-dev.json"
    } else {
        "marketplace_custom.json"
    };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(filename)
}

pub fn load_custom() -> Vec<MarketplacePlugin> {
    let p = custom_cache_path();
    if !p.exists() { return Vec::new(); }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_custom(plugins: &[MarketplacePlugin]) {
    let p = custom_cache_path();
    if let Some(parent) = p.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("custom cache create_dir_all failed: {e}");
            return;
        }
    }
    match serde_json::to_string_pretty(plugins) {
        Ok(s) => {
            if let Err(e) = std::fs::write(&p, s) {
                tracing::warn!("custom cache write {p:?} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("custom cache serialise failed: {e}"),
    }
}

#[allow(dead_code)] // Phase 5 polish will use this for the "purge cache" action.
pub fn invalidate_custom() {
    let _ = std::fs::remove_file(custom_cache_path());
}
