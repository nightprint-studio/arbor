//! Persisted record of which marketplace entries are currently installed.
//!
//! This file is the marketplace's source-of-truth for the `installed` flag
//! — NOT the host's `plugins/` directory. Dev plugins and plugins copied in
//! by hand are a separate concern: they may or may not appear on disk but
//! they never show up in the marketplace UI as "installed" unless they
//! were actually downloaded through it.
//!
//! Lives at `~/.config/arbor/marketplace_installed.json` (debug uses a
//! `-dev` suffix so a side-by-side dev Arbor doesn't poison the prod
//! install ledger).

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::types::RegistryEntry;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstalledFile {
    #[serde(default)] pub plugins: HashMap<String, InstalledPlugin>,
    #[serde(default)] pub themes:  HashMap<String, InstalledTheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub name:         String,
    pub version:      String,
    pub entry:        RegistryEntry,
    /// Commit SHA we resolved at install time (when GitHub returned one).
    /// Phase 4 can use it to compare against the registry's `pinned_sha` for
    /// custom sources.
    pub resolved_sha: Option<String>,
    /// Absolute path to the install directory on disk.
    pub install_path: String,
    pub installed_at: u64,
    /// Mirror of the host's enable state. Marketplace owns this for plugins
    /// it installed; the host's `plugin_states[-dev].json` is still updated
    /// in lock-step so existing tooling (Plugin Manager toggle) keeps working.
    pub enabled:      bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledTheme {
    pub id:           String,
    pub name:         String,
    pub entry:        RegistryEntry,
    pub install_path: String,
    pub installed_at: u64,
}

// ---------------------------------------------------------------------------
// File location
// ---------------------------------------------------------------------------

pub fn path() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "marketplace_installed-dev.json"
    } else {
        "marketplace_installed.json"
    };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(filename)
}

// ---------------------------------------------------------------------------
// Read / write
// ---------------------------------------------------------------------------

pub fn load() -> InstalledFile {
    let p = path();
    if !p.exists() { return InstalledFile::default(); }
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(file: &InstalledFile) {
    let p = path();
    if let Some(parent) = p.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!("marketplace installs: create_dir_all failed: {e}");
            return;
        }
    }
    match serde_json::to_string_pretty(file) {
        Ok(s) => {
            if let Err(e) = std::fs::write(&p, s) {
                tracing::warn!("marketplace installs write {p:?} failed: {e}");
            }
        }
        Err(e) => tracing::warn!("marketplace installs serialise failed: {e}"),
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Convenience helpers used by the registry + commands
// ---------------------------------------------------------------------------

pub fn record_plugin(p: InstalledPlugin) {
    let mut file = load();
    file.plugins.insert(p.name.clone(), p);
    save(&file);
}

pub fn forget_plugin(name: &str) -> Option<InstalledPlugin> {
    let mut file = load();
    let removed = file.plugins.remove(name);
    if removed.is_some() { save(&file); }
    removed
}

pub fn record_theme(t: InstalledTheme) {
    let mut file = load();
    file.themes.insert(t.id.clone(), t);
    save(&file);
}

pub fn forget_theme(id: &str) -> Option<InstalledTheme> {
    let mut file = load();
    let removed = file.themes.remove(id);
    if removed.is_some() { save(&file); }
    removed
}

pub fn set_plugin_enabled(name: &str, enabled: bool) {
    let mut file = load();
    if let Some(p) = file.plugins.get_mut(name) {
        p.enabled = enabled;
        save(&file);
    }
}

// ---------------------------------------------------------------------------
// On-disk install directory (separate from the host's plugins/ dev dir)
// ---------------------------------------------------------------------------

/// Directory marketplace-installed plugins live in. Kept intentionally
/// separate from the host's `plugin_dir()` so dev plugins (or hand-copied
/// folders) never collide with marketplace installs.
pub fn marketplace_plugin_dir() -> PathBuf {
    let filename = if cfg!(debug_assertions) {
        "marketplace_plugins-dev"
    } else {
        "marketplace_plugins"
    };
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join(filename)
}
