//! `plugin.toml` manifest shape + pure parsing entry point. Discovery,
//! filesystem walking, and the persisted enable/disable state file live in
//! the host (`arbor` / `arbor-plugin-core`); this crate intentionally stays
//! free of I/O.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::dependency::Dependency;
use crate::hooks::Hooks;
use crate::permissions::Permissions;
use crate::sandbox::Sandbox;
use crate::schedule::SchedulerSection;

// ---------------------------------------------------------------------------
// Manifest (plugin.toml)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    // ── Identity ──────────────────────────────────────────────────────────────
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub author:      String,
    #[serde(default)]
    pub license:     Option<String>,
    #[serde(default)]
    pub repository:  Option<String>,
    /// Optional homepage / docs URL — surfaced in the marketplace detail pane.
    #[serde(default)]
    pub homepage:    Option<String>,
    #[serde(default)]
    pub keywords:    Vec<String>,
    /// Free-form category for marketplace filtering. Curated values:
    /// `build` · `ci` · `git-workflow` · `language` · `ui` · `data` · `theme`.
    #[serde(default)]
    pub category:    Option<String>,
    /// Path (relative to the plugin dir) to a square SVG/PNG icon used by the
    /// marketplace + Plugin Manager. Falls back to a monogram when absent.
    #[serde(default)]
    pub icon:        Option<String>,

    // ── Compatibility ─────────────────────────────────────────────────────────
    /// Minimum Arbor version required (semver string), e.g. "0.8.0".
    /// Validated against `ARBOR_APP_VERSION` at load time — incompatible
    /// plugins are rejected with a clear error.
    #[serde(default)]
    pub min_arbor_version: Option<String>,
    /// Integer version of the Lua API contract. Bumped on breaking changes.
    /// Plugins with arbor_api > ARBOR_API_VERSION are rejected at load time.
    #[serde(default = "default_arbor_api")]
    pub arbor_api: u32,
    /// Operating systems this plugin supports. Empty = cross-platform.
    /// Recognised values: "windows", "linux", "macos". Plugins running on a
    /// non-listed OS are skipped at discovery time.
    #[serde(default)]
    pub os: Vec<String>,
    /// Plugin entry point. Defaults to "main.lua".
    #[serde(default = "default_entry")]
    pub entry: String,

    // ── Documentation ────────────────────────────────────────────────────────
    /// Optional path to an HTML file (relative to plugin dir) shown in the
    /// Docs panel under "Plugins". Not required — omit to skip the Plugins section.
    #[serde(default)]
    pub doc_file: Option<String>,

    /// When true, the plugin is flagged as experimental in the Plugin Manager
    /// (orange "EXPERIMENTAL" pill next to the version). Intended for plugins
    /// that are still iterating heavily on their public surface — settings,
    /// hooks, storage formats — and may break between releases.
    #[serde(default)]
    pub experimental: bool,

    // ── Sections ──────────────────────────────────────────────────────────────
    pub permissions: Permissions,
    #[serde(default)]
    pub sandbox:     Sandbox,
    #[serde(default)]
    pub hooks:       Hooks,
    /// Background-scheduler opt-in. Schedule data (interval / cron / etc.)
    /// is declared from Lua via `arbor.scheduler.register`; the manifest only
    /// gates the feature on or off.
    #[serde(default)]
    pub scheduler:   SchedulerSection,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,

    /// Path to the plugin directory — not in TOML, filled at discovery time.
    #[serde(skip)]
    pub dir: PathBuf,
}

fn default_arbor_api() -> u32 { 1 }
fn default_entry() -> String { "main.lua".to_string() }

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Errors that can occur when parsing a `plugin.toml` text payload.
#[derive(Debug, Error)]
pub enum ManifestParseError {
    #[error("plugin.toml parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

impl Manifest {
    /// Parse a `plugin.toml` text payload and stamp the plugin directory on the
    /// resulting [`Manifest`]. No filesystem access — callers (the host's
    /// `discover_plugins`, the marketplace installer, etc.) read the file and
    /// pass the resulting string in.
    pub fn from_toml_str(content: &str, dir: &Path) -> Result<Self, ManifestParseError> {
        let mut manifest: Manifest = toml::from_str(content)?;
        manifest.dir = dir.to_path_buf();
        Ok(manifest)
    }
}

// ---------------------------------------------------------------------------
// Discovery-time failure record
// ---------------------------------------------------------------------------

/// A plugin folder whose `plugin.toml` could not be parsed. Kept separate
/// from `Manifest` because we don't have a typed manifest to attach the error
/// to — the folder name is the best stand-in for the plugin name.
#[derive(Debug, Clone)]
pub struct ManifestParseFailure {
    pub folder_name: String,
    pub error:       String,
}
