//! Leaf-level fetchers: resolve a single plugin or theme entry into the
//! `MarketplacePlugin` / `MarketplaceTheme` DTO.
//!
//! Composed by [`crate::index::fetch_catalog`] for the curated catalog and
//! by [`crate::custom::resolve_custom_source`] for user-added sources.

use serde::Deserialize;

use arbor_plugin_types::prelude::Manifest;

use crate::error::{MarketplaceError, Result};
use crate::github_api::{github_url, join_subpath, raw_url};
use crate::types::{
    MarketplacePlugin, MarketplaceSource, MarketplaceTheme, MarketplaceThemePreview,
    RegistryEntry, ThemeVariant,
};

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
// Plugin entry
// ---------------------------------------------------------------------------

pub async fn fetch_plugin(
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
        .map_err(|e| MarketplaceError::Other(format!("GET {toml_url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {toml_url}: {e}")))?
        .text().await
        .map_err(|e| MarketplaceError::Other(format!("body {toml_url}: {e}")))?;
    let manifest: Manifest = toml::from_str(&body)
        .map_err(|e| MarketplaceError::Other(format!("parse {toml_url}: {e}")))?;

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
            external:   false,
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

pub async fn fetch_theme(
    http:    &reqwest::Client,
    owner:   &str,
    repo:    &str,
    r#ref:   &str,
    subpath: &str,
    source:  MarketplaceSource,
) -> Result<MarketplaceTheme> {
    let url = raw_url(owner, repo, r#ref, subpath);
    let raw: RawThemeFile = http.get(&url).send().await
        .map_err(|e| MarketplaceError::Other(format!("GET {url}: {e}")))?
        .error_for_status()
        .map_err(|e| MarketplaceError::Other(format!("HTTP {url}: {e}")))?
        .json().await
        .map_err(|e| MarketplaceError::Other(format!("parse {url}: {e}")))?;

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
            external:   false,
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
