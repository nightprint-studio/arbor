//! Real GitHub fetcher for the marketplace catalog.
//!
//! Hits `https://raw.githubusercontent.com/{owner}/{repo}/main/...` for the
//! curated `arbor-extensions` repo (and any custom-source repo of the same
//! shape):
//!
//!   1. `index.json` at the repo root — pointer list of plugins + themes.
//!   2. Per-plugin `plugin.toml` + optional icon SVG + optional doc HTML.
//!   3. Per-theme JSON file.
//!
//! Failures on a single entry are logged and skipped, not propagated — a
//! bad community submission shouldn't take down the whole catalog. The
//! caller (`MarketplaceRegistry::refresh`) decides what to do with the
//! per-entry log: surface a partial-result banner, retry, etc.

use std::time::Duration;

use futures_util::future::join_all;
use serde::Deserialize;

use crate::error::{AppError, Result};
use crate::plugin::runtime::manifest::PluginManifest;

use super::types::{
    MarketplaceCatalog, MarketplacePlugin, MarketplaceSource, MarketplaceTheme,
    MarketplaceThemePreview, RegistryEntry, ThemeVariant,
};

/// Curated registry — flipped here so adding a mirror later is a one-line
/// change. Custom user sources pass their own URL to `fetch_catalog`.
pub const REGISTRY_REPO: &str = "https://github.com/nightprint-studio/arbor-extensions";

/// We pin to `main` for now per design decision — tag-based resolution will
/// land once `arbor-extensions` has its first tagged release.
pub const REGISTRY_REF: &str = "main";

const RAW_HOST: &str = "https://raw.githubusercontent.com";
/// Per-request timeout. Generous enough for a slow GitHub edge but short
/// enough that the modal doesn't feel stuck.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(concat!("arbor/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| AppError::Other(format!("marketplace HTTP client init failed: {e}")))
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

/// Parse `https://github.com/{owner}/{repo}[.git]` → (owner, repo).
pub fn parse_github_repo(url: &str) -> Option<(String, String)> {
    let stripped = url.trim_end_matches('/').trim_end_matches(".git");
    let suffix = stripped
        .strip_prefix("https://github.com/")
        .or_else(|| stripped.strip_prefix("http://github.com/"))?;
    let mut parts = suffix.split('/');
    let owner = parts.next()?;
    let repo  = parts.next()?;
    if owner.is_empty() || repo.is_empty() { return None; }
    Some((owner.to_string(), repo.to_string()))
}

fn raw_url(owner: &str, repo: &str, r#ref: &str, path: &str) -> String {
    let p = path.trim_start_matches('/');
    format!("{RAW_HOST}/{owner}/{repo}/{}/{p}", r#ref)
}

fn join_subpath(subpath: &str, file: &str) -> String {
    let s = subpath.trim_end_matches('/');
    let f = file.trim_start_matches('/');
    if s.is_empty() { f.to_string() } else { format!("{s}/{f}") }
}

// ---------------------------------------------------------------------------
// index.json
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct IndexFile {
    #[serde(default)] plugins: Vec<IndexEntry>,
    #[serde(default)] themes:  Vec<IndexEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct IndexEntry {
    subpath: String,
    #[serde(default)] #[serde(rename = "ref")]
    r#ref:   Option<String>,
}

// ---------------------------------------------------------------------------
// Theme JSON shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RawThemeFile {
    id:          String,
    name:        String,
    #[serde(default)] description: Option<String>,
    #[serde(default)] author:      Option<String>,
    #[serde(default)] variant:     Option<ThemeVariant>,
    #[serde(default)] tags:        Option<Vec<String>>,
    #[serde(default)] vars:        std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Public fetch entry point
// ---------------------------------------------------------------------------

/// Fetch the catalog from a GitHub-hosted registry. Per-entry failures are
/// logged and dropped so a single bad submission doesn't blank the catalog.
pub async fn fetch_catalog(
    http:        &reqwest::Client,
    repo_url:    &str,
    source_kind: MarketplaceSource,
) -> Result<MarketplaceCatalog> {
    let (owner, repo) = parse_github_repo(repo_url)
        .ok_or_else(|| AppError::Other(format!("invalid GitHub repo URL: {repo_url}")))?;

    let index_url = raw_url(&owner, &repo, REGISTRY_REF, "index.json");
    tracing::info!("marketplace: fetching index from {index_url}");

    let index: IndexFile = http
        .get(&index_url)
        .send().await
        .map_err(|e| AppError::Other(format!("fetch {index_url}: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("HTTP {index_url}: {e}")))?
        .json().await
        .map_err(|e| AppError::Other(format!("parse {index_url}: {e}")))?;

    // Plugins — fetched in parallel.
    let plugin_futs = index.plugins.iter().cloned().map(|entry| {
        let http  = http.clone();
        let owner = owner.clone();
        let repo  = repo.clone();
        let src   = source_kind;
        async move {
            let r#ref = entry.r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string());
            fetch_plugin(&http, &owner, &repo, &r#ref, &entry.subpath, src).await
        }
    });
    let theme_futs = index.themes.iter().cloned().map(|entry| {
        let http  = http.clone();
        let owner = owner.clone();
        let repo  = repo.clone();
        let src   = source_kind;
        async move {
            let r#ref = entry.r#ref.clone().unwrap_or_else(|| REGISTRY_REF.to_string());
            fetch_theme(&http, &owner, &repo, &r#ref, &entry.subpath, src).await
        }
    });

    let (plugin_results, theme_results) = tokio::join!(
        join_all(plugin_futs),
        join_all(theme_futs),
    );

    let mut plugins: Vec<MarketplacePlugin> = plugin_results.into_iter()
        .filter_map(|r| match r {
            Ok(p)  => Some(p),
            Err(e) => { tracing::warn!("marketplace plugin entry skipped: {e}"); None }
        })
        .collect();
    let mut themes: Vec<MarketplaceTheme> = theme_results.into_iter()
        .filter_map(|r| match r {
            Ok(t)  => Some(t),
            Err(e) => { tracing::warn!("marketplace theme entry skipped: {e}"); None }
        })
        .collect();

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    themes .sort_by(|a, b| a.name.cmp(&b.name));

    Ok(MarketplaceCatalog { plugins, themes })
}

// ---------------------------------------------------------------------------
// Plugin entry
// ---------------------------------------------------------------------------

async fn fetch_plugin(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
    source:  MarketplaceSource,
) -> Result<MarketplacePlugin> {
    let toml_path = join_subpath(subpath, "plugin.toml");
    let toml_url  = raw_url(owner, repo, r#ref, &toml_path);

    let body = http.get(&toml_url).send().await
        .map_err(|e| AppError::Other(format!("GET {toml_url}: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("HTTP {toml_url}: {e}")))?
        .text().await
        .map_err(|e| AppError::Other(format!("body {toml_url}: {e}")))?;
    let manifest: PluginManifest = toml::from_str(&body)
        .map_err(|e| AppError::Other(format!("parse {toml_url}: {e}")))?;

    // Optional icon SVG. We inline the file content so the modal can theme
    // it with `currentColor`. Binary icons (PNG) fall back to the raw URL.
    let icon = match manifest.icon.as_deref() {
        Some(rel) => fetch_icon(http, owner, repo, r#ref, subpath, rel).await,
        None      => None,
    };

    // Optional HTML doc — same path treatment as the host's DocsPanel.
    let doc = match manifest.doc_file.as_deref() {
        Some(rel) => fetch_text(http, owner, repo, r#ref, &join_subpath(subpath, rel)).await,
        None      => None,
    };

    Ok(MarketplacePlugin {
        name:        manifest.name,
        version:     manifest.version,
        description: manifest.description,
        author:      manifest.author,
        category:    manifest.category,
        tags:        if manifest.keywords.is_empty() { None } else { Some(manifest.keywords) },
        repository:  manifest.repository.or_else(|| Some(github_url(owner, repo))),
        homepage:    manifest.homepage,
        min_arbor_version: manifest.min_arbor_version,
        icon,
        screenshots: None,
        permissions: Some(manifest.permissions),
        source,
        installed:   false,
        enabled:     None,
        entry: RegistryEntry {
            repo:       github_url(owner, repo),
            r#ref:      Some(r#ref.to_string()),
            subpath:    Some(subpath.to_string()),
            source,
            pinned_sha: None,
        },
        experimental: if manifest.experimental { Some(true) } else { None },
        doc,
        update_available:  None,
        installed_version: None,
        dependencies: manifest.dependencies,
    })
}

async fn fetch_icon(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
    rel:     &str,
) -> Option<String> {
    let icon_path = join_subpath(subpath, rel);
    let icon_url  = raw_url(owner, repo, r#ref, &icon_path);

    let resp = http.get(&icon_url).send().await.ok()?.error_for_status().ok()?;
    if rel.to_ascii_lowercase().ends_with(".svg") {
        // SVG → inline so the modal can paint with currentColor.
        resp.text().await.ok()
    } else {
        // Non-SVG (PNG, …) → just keep the raw URL.
        Some(icon_url)
    }
}

async fn fetch_text(
    http:  &reqwest::Client,
    owner: &str,
    repo:  &str,
    r#ref: &str,
    path:  &str,
) -> Option<String> {
    let url  = raw_url(owner, repo, r#ref, path);
    let resp = http.get(&url).send().await.ok()?.error_for_status().ok()?;
    resp.text().await.ok()
}

// ---------------------------------------------------------------------------
// Theme entry
// ---------------------------------------------------------------------------

async fn fetch_theme(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
    source:  MarketplaceSource,
) -> Result<MarketplaceTheme> {
    let url = raw_url(owner, repo, r#ref, subpath);
    let raw: RawThemeFile = http.get(&url).send().await
        .map_err(|e| AppError::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::Other(format!("HTTP {url}: {e}")))?
        .json().await
        .map_err(|e| AppError::Other(format!("parse {url}: {e}")))?;

    let pick = |k: &str| raw.vars.get(k).cloned().unwrap_or_else(|| "#000000".into());
    let preview = MarketplaceThemePreview {
        bg:      pick("--bg-base"),
        fg:      pick("--text-primary"),
        accent:  pick("--accent"),
        success: pick("--success"),
        warning: pick("--warning"),
        error:   pick("--error"),
    };

    // Variant guess: respect explicit field; otherwise sniff the id.
    let variant = raw.variant.or_else(|| Some(guess_variant(&raw.id)));

    Ok(MarketplaceTheme {
        id:          raw.id,
        name:        raw.name,
        description: raw.description.unwrap_or_default(),
        author:      raw.author,
        tags:        raw.tags,
        preview,
        variant,
        source,
        installed:   false,
        entry: RegistryEntry {
            repo:       github_url(owner, repo),
            r#ref:      Some(r#ref.to_string()),
            subpath:    Some(subpath.to_string()),
            source,
            pinned_sha: None,
        },
    })
}

fn guess_variant(id: &str) -> ThemeVariant {
    let lc = id.to_ascii_lowercase();
    if lc.contains("light") || lc.contains("day") || lc.contains("dawn") || lc.contains("latte") {
        ThemeVariant::Light
    } else {
        ThemeVariant::Dark
    }
}

fn github_url(owner: &str, repo: &str) -> String {
    format!("https://github.com/{owner}/{repo}")
}

// ---------------------------------------------------------------------------
// Custom-source resolver (Phase 4)
// ---------------------------------------------------------------------------

/// Outcome of a user-added custom source resolution. A single repo can
/// point at one plugin (root or subpath modes) or at a multi-plugin index
/// (`index.json` at root), so the result is split into two shapes.
#[derive(Debug)]
pub enum CustomSourceResolution {
    /// Single plugin — root mode (`plugin.toml` at repo root) or subpath
    /// mode (`{subpath}/plugin.toml`).
    Single(MarketplacePlugin),
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
        .ok_or_else(|| AppError::Other(format!(
            "'{repo_url}' is not a recognised GitHub URL — expected \
             https://github.com/{{owner}}/{{repo}}"
        )))?;
    let ref_str = r#ref.unwrap_or(REGISTRY_REF);

    // Mode 1 — explicit subpath wins.
    if let Some(sp) = subpath.filter(|s| !s.is_empty()) {
        let plugin = fetch_custom_plugin(http, &owner, &repo, ref_str, sp).await
            .map_err(|e| AppError::Other(format!(
                "subpath mode failed for '{repo_url}' @ '{sp}': {e}"
            )))?;
        return Ok(CustomSourceResolution::Single(plugin));
    }

    // Mode 2 — single plugin at root.
    let root_toml = raw_url(&owner, &repo, ref_str, "plugin.toml");
    if probe(http, &root_toml).await {
        let plugin = fetch_custom_plugin(http, &owner, &repo, ref_str, "").await
            .map_err(|e| AppError::Other(format!("root mode failed: {e}")))?;
        return Ok(CustomSourceResolution::Single(plugin));
    }

    // Mode 3 — multi-plugin index at root.
    let root_index = raw_url(&owner, &repo, ref_str, "index.json");
    if probe(http, &root_index).await {
        let catalog = fetch_catalog(http, repo_url, MarketplaceSource::Custom).await?;
        return Ok(CustomSourceResolution::Multi(catalog.plugins));
    }

    Err(AppError::Other(format!(
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
