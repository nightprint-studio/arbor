//! Serializable DTOs shared with the frontend `MarketplaceModal`.
//!
//! The shapes mirror `src/lib/types/marketplace.ts` 1:1 — anything renamed
//! here must be renamed there at the same time. The TS file is the source of
//! truth for the field set; this file just ports it to Rust so Tauri commands
//! can return native types and serde handles the wire format.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use arbor_plugin_types::prelude::{CredentialSlot, Dependency, Permissions, Provides};

// ---------------------------------------------------------------------------
// Where a listing came from
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarketplaceSource {
    /// First-party entry that lives inside the curated registry repo
    /// itself (Internal `index.json` entry). Authored by the registry
    /// maintainers — strongest provenance available.
    Official,
    /// External entry — listed in the curated registry (vetted via PR
    /// review) but the source lives in a third-party GitHub repo. The
    /// registry vouches, but the code isn't under the maintainers' direct
    /// control.
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
    /// True when this entry points at a third-party repo (different from the
    /// registry repo it was listed in). `source = Community` stays the same —
    /// vetting still happens via PR review on the registry — but the FE
    /// uses this flag to surface a "this lives in someone else's repo" hint
    /// and to encourage `pinned_sha` for moving refs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external:    bool,
    /// `file name → sha256` for the release assets this entry approves.
    ///
    /// Empty for an ordinary Lua plugin, which installs from a source archive and whose
    /// integrity story is `pinned_sha` — a git SHA is content-addressed, so pinning the
    /// commit pins the source. A package that carries a build artifact has no such luxury:
    /// a `.wasm` is not in the tree the commit names, so the registry records the digest of
    /// the exact file a reviewer approved, and any later substitution fails to install.
    ///
    /// Recorded **in the registry** rather than in the package's own manifest on purpose: a
    /// digest supplied by the author verifies only that the author is consistent with
    /// themselves. This is the one place a human signed off.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub artifacts:   BTreeMap<String, String>,
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
    pub permissions: Option<Permissions>,
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
    /// Direct plugin-to-plugin dependencies declared in the resolved
    /// `plugin.toml`. Empty when the plugin stands alone. Surfaced in the
    /// install-confirm modal so the user can pre-install required deps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<Dependency>,
    /// Credential slots the package declares. Surfaced in the install-confirm dialog so the
    /// consent says **what** will be stored — "this plugin uses credentials" is a formality,
    /// "this plugin stores your Google account token" is a question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<CredentialSlot>,
    /// Host interfaces the package implements. Non-empty means it carries a compiled module,
    /// which is the one thing about a listing a user cannot discover by reading its source.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub provides: Vec<Provides>,
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
