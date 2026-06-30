//! Resolver for user-added custom sources.
//!
//! A user supplies a GitHub URL (+ optional ref / subpath / pinned_sha)
//! and the resolver tries three modes in order — subpath, root, multi —
//! to decide whether the URL hosts a single plugin or a multi-plugin
//! `index.json`. The result is folded back into the catalog with
//! `MarketplaceSource::Custom`.

use crate::error::{MarketplaceError, Result};
use crate::fetch::fetch_plugin;
use crate::github_api::{parse_github_repo, raw_url};
use crate::index::{fetch_catalog, REGISTRY_REF};
use crate::types::{MarketplacePlugin, MarketplaceSource};

/// Outcome of a user-added custom source resolution. A single repo can
/// point at one plugin (root or subpath modes) or at a multi-plugin index
/// (`index.json` at root), so the result is split into two shapes.
#[derive(Debug)]
pub enum CustomSourceResolution {
    /// Single plugin — root mode (`plugin.toml` at repo root) or subpath
    /// mode (`{subpath}/plugin.toml`). Boxed so this variant doesn't bloat
    /// the enum next to the small `Multi` (a `Vec`).
    Single(Box<MarketplacePlugin>),
    /// Multi-plugin: the repo hosts an `index.json` listing several
    /// plugins (and possibly themes). The themes are dropped here — only
    /// plugins are surfaced for custom-source mode.
    Multi(Vec<MarketplacePlugin>),
}

/// Resolve a user-supplied repo URL into one or more `MarketplacePlugin`
/// entries. Tries three modes in order:
///
///   1. **Subpath mode** — when the caller supplies `subpath`, we fetch
///      `{subpath}/plugin.toml` directly. Useful for picking a single
///      plugin out of a multi-plugin repo without going through the
///      index.
///   2. **Root mode** — `plugin.toml` at the repo root → single plugin.
///   3. **Multi mode** — `index.json` at the repo root → run the regular
///      community-style fetcher with `source = Custom`.
///
/// Errors out (with a human-readable message) when none of the three
/// match — the FE surfaces this in the Add-source form.
pub async fn resolve_custom_source(
    http:     &reqwest::Client,
    repo_url: &str,
    r#ref:    Option<&str>,
    subpath:  Option<&str>,
) -> Result<CustomSourceResolution> {
    let (owner, repo) = parse_github_repo(repo_url)
        .ok_or_else(|| MarketplaceError::InvalidUrl(repo_url.to_string()))?;
    let ref_str = r#ref.unwrap_or(REGISTRY_REF);

    // Mode 1 — explicit subpath wins.
    if let Some(sp) = subpath.filter(|s| !s.is_empty()) {
        let plugin = fetch_custom_plugin(http, &owner, &repo, ref_str, sp).await
            .map_err(|e| MarketplaceError::Other(format!(
                "subpath mode failed for '{repo_url}' @ '{sp}': {e}"
            )))?;
        return Ok(CustomSourceResolution::Single(Box::new(plugin)));
    }

    // Mode 2 — single plugin at root.
    let root_toml = raw_url(&owner, &repo, ref_str, "plugin.toml");
    if probe(http, &root_toml).await {
        let plugin = fetch_custom_plugin(http, &owner, &repo, ref_str, "").await
            .map_err(|e| MarketplaceError::Other(format!("root mode failed: {e}")))?;
        return Ok(CustomSourceResolution::Single(Box::new(plugin)));
    }

    // Mode 3 — multi-plugin index at root.
    let root_index = raw_url(&owner, &repo, ref_str, "index.json");
    if probe(http, &root_index).await {
        let catalog = fetch_catalog(http, repo_url, MarketplaceSource::Custom).await?;
        return Ok(CustomSourceResolution::Multi(catalog.plugins));
    }

    Err(MarketplaceError::Other(format!(
        "no plugin.toml at root, no index.json at root, and no subpath \
         supplied — repo '{repo_url}' does not look like an Arbor plugin source"
    )))
}

/// Send a HEAD-ish request and report whether the resource resolves. We
/// use GET because GitHub's raw host returns 200/404 reliably for GETs;
/// HEAD support is spottier on the CDN.
async fn probe(http: &reqwest::Client, url: &str) -> bool {
    match http.get(url).send().await {
        Ok(r)  => r.status().is_success(),
        Err(_) => false,
    }
}

/// Like `fetch_plugin` but tags the result as `MarketplaceSource::Custom`
/// and uses the user-supplied repo URL verbatim (so the resolved
/// `RegistryEntry::repo` matches what the user typed, not the
/// `github.com/...` canonical we constructed internally).
async fn fetch_custom_plugin(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
) -> Result<MarketplacePlugin> {
    let mut p = fetch_plugin(http, owner, repo, r#ref, subpath, MarketplaceSource::Custom).await?;
    // `fetch_plugin` sets `entry.subpath = Some("")` when subpath is empty —
    // normalise to `None` so the wire format is cleaner for root-mode entries.
    if p.entry.subpath.as_deref() == Some("") {
        p.entry.subpath = None;
    }
    Ok(p)
}
