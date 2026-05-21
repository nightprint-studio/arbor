//! Serializable DTOs shared with the frontend `MarketplaceModal`.
//!
//! The shapes mirror `src/lib/types/marketplace.ts` 1:1 — anything renamed
//! here must be renamed there at the same time. The TS file is the source of
//! truth for the field set; this file just ports it to Rust so Tauri commands
//! can return native types and serde handles the wire format.

use serde::{Deserialize, Serialize};

use crate::plugin::runtime::manifest::permissions::PluginPermissions;

// ---------------------------------------------------------------------------
// Where a listing came from
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceSource {
    /// Listed in the curated `arbor-extensions` repo (vetted via PR review).
    Community,
    /// User-supplied git URL — third-party, unverified.
    Custom,
    /// Plugin lives in the host's plugins/ folder but has no matching
    /// marketplace entry (zip sideload, dev folder, …).
    Local,
}

// ---------------------------------------------------------------------------
// Pointer entry (the `index.json` row)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// `https://github.com/<owner>/<repo>` — GitHub-only for now.
    pub repo:        String,
    /// Git ref (tag, branch, SHA). Empty / None → resolver picks the latest tag
    /// and falls back to `main`.
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub r#ref:       Option<String>,
    /// Subpath inside the repo for multi-plugin / multi-theme repos.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath:     Option<String>,
    pub source:      MarketplaceSource,
    /// Optional commit SHA pin — installer refuses to proceed if the resolved
    /// ref doesn't match. Defends custom sources against tag-hijack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned_sha:  Option<String>,
}

// ---------------------------------------------------------------------------
// Resolved plugin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags:        Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository:  Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage:    Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_arbor_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon:        Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshots: Option<Vec<String>>,
    /// `[permissions]` block from the resolved `plugin.toml`. Re-uses the
    /// existing host type so the same JSON-on-wire shape the Plugin Manager
    /// already speaks works here too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PluginPermissions>,
    pub source:      MarketplaceSource,
    pub installed:   bool,
    /// Mirror of the host's enable state when `installed = true`. Undefined
    /// (None) when not installed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled:     Option<bool>,
    pub entry:       RegistryEntry,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<bool>,
    /// Authored HTML doc — sourced from `plugin.toml`'s `doc_file`. Rendered
    /// inside the modal with DocsPanel-style chrome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc:         Option<String>,
    /// When set, the installed version is older than the catalog version
    /// and the user can hit "Update" to re-run the install path. Carries
    /// the newer version string for display ("v1.2 → v1.3").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_available: Option<String>,
    /// Version currently on disk (from `marketplace_installed.json`). Same
    /// as `version` when no update is available; older when one is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
}

// ---------------------------------------------------------------------------
// Resolved theme
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceThemePreview {
    pub bg:      String,
    pub fg:      String,
    pub accent:  String,
    pub success: String,
    pub warning: String,
    pub error:   String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeVariant {
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceTheme {
    pub id:          String,
    pub name:        String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author:      Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags:        Option<Vec<String>>,
    pub preview:     MarketplaceThemePreview,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant:     Option<ThemeVariant>,
    pub source:      MarketplaceSource,
    pub installed:   bool,
    pub entry:       RegistryEntry,
}

// ---------------------------------------------------------------------------
// Aggregated catalog returned by `marketplace_fetch_registry`
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceCatalog {
    pub plugins: Vec<MarketplacePlugin>,
    pub themes:  Vec<MarketplaceTheme>,
}
