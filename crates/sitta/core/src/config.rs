//! `config` — the typed **global** sitta (file-explorer) configuration
//! (`arbor/profiles/<active>/sitta/config.toml`, per-profile) owned
//! **out-of-process** by `sitta-be`.
//!
//! Holds the file-explorer's own UX preferences — the subset of the old shell
//! `ExplorerConfig` that the explorer FE consumes (view/sort/startup, sidebar +
//! column layout, favourites, saved searches, external-link policy, the
//! git-awareness master switch). The four settings the **shell** still consumes
//! (the OS-global shortcut + accelerator, `always_new_window`, `reveal_in_builtin`)
//! deliberately stayed in the launcher config — they are window/OS-integration
//! policy the launcher reads even when sitta-be isn't running.
//!
//! Like `merula-core`'s config, the path is **not** pushed by the shell: sitta-be
//! resolves [`sitta_config_path`](arbor_core::prelude::sitta_config_path) itself,
//! since `init_active_profile()` ran in `main` before any handler is served.
//!
//! [`load`] is infallible-by-design: a missing / unparseable file yields
//! [`SittaConfig::default`] so operational reads never break. The
//! `get/set_sitta_config` handlers stay in sitta-be and call back into [`load`] /
//! [`save`] here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Persisted sitta settings (global, per-profile `…/sitta/config.toml`).
///
/// Field order matters for TOML serialization: every scalar / value-array field is
/// declared before the array-of-tables fields (`sidebar_sections` / `columns` /
/// `saved_searches`), or `toml` fails with "values must be emitted before tables".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SittaConfig {
    /// Master switch for TortoiseGit-style git awareness in the file explorer
    /// (status overlays, repo-root markers, the Changes panel, branch switch).
    /// Off by default — when off the explorer issues no git IPC at all.
    pub git_awareness: bool,
    /// Default view mode applied to not-yet-visited folders:
    /// `details` | `medium` | `large` | `xlarge`.
    pub default_view: String,
    /// Show dot-prefixed (hidden) entries by default.
    pub show_hidden: bool,
    /// Default state of recursive (subfolder) search.
    pub recursive_search: bool,
    /// Default column the listing sorts by: `name` | `modified` | `size`.
    pub default_sort: String,
    /// Default sort direction (ascending when true).
    pub sort_ascending: bool,
    /// What a freshly-opened explorer tab shows: `overview` (the dashboard) or
    /// `last` (re-open the most recent folder, if any).
    pub startup: String,
    /// Maximum number of recent folders kept in the sidebar (clamped 1–50).
    pub max_recents: u32,
    /// Allow opening generic external links typed in the explorer address bar
    /// (custom schemes like `vscode://`, `mailto:`, `slack://`) via the OS default
    /// handler. Off by default. `arbor://` deep links are handled separately.
    pub open_external_links: bool,
    /// Additionally allow plain web links (`http://`, `https://`) from the address
    /// bar to open in the default browser. Gated behind `open_external_links`.
    pub open_web_links: bool,
    /// Schemes the user chose "remember" for in the external-link confirm prompt
    /// (lower-cased, e.g. `["vscode", "https"]`).
    pub remembered_external_schemes: Vec<String>,
    /// User-pinned favourite folders shown in the sidebar's Favourites section, in
    /// addition to the OS standard locations. Absolute paths.
    pub pinned_favourites: Vec<String>,
    /// Sidebar section order + visibility. Empty → built-in order, all shown.
    pub sidebar_sections: Vec<ExplorerSectionConfig>,
    /// Details-view column order + visibility. Empty → built-in order with the
    /// default-on set shown. `name` is always shown first regardless.
    pub columns: Vec<ExplorerColumnConfig>,
    /// Saved searches surfaced as their own sidebar section. Each captures a query
    /// + filters + (optional) root folder and re-runs on click.
    pub saved_searches: Vec<ExplorerSavedSearch>,
}

impl Default for SittaConfig {
    fn default() -> Self {
        Self {
            git_awareness:               false,
            default_view:                "details".to_string(),
            show_hidden:                 false,
            recursive_search:            false,
            default_sort:                "name".to_string(),
            sort_ascending:              true,
            startup:                     "overview".to_string(),
            max_recents:                 10,
            open_external_links:         false,
            open_web_links:              false,
            remembered_external_schemes: Vec::new(),
            pinned_favourites:           Vec::new(),
            sidebar_sections:            Vec::new(),
            columns:                     Vec::new(),
            saved_searches:              Vec::new(),
        }
    }
}

/// One sidebar section's persisted order + visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerSectionConfig {
    /// Section id: `library` | `recents` | `favourites` | `devices` | `projects`.
    pub id: String,
    /// Whether the section is shown.
    pub visible: bool,
}

/// One details-view column's persisted order + visibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerColumnConfig {
    /// Column id: `name` | `modified` | `type` | `size` | `created` |
    /// `extension` | `gitstatus`.
    pub id: String,
    /// Whether the column is shown.
    pub visible: bool,
}

/// A saved search: a query plus the advanced filters and (optional) root it was
/// captured with. The frontend owns filter semantics; this is opaque storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerSavedSearch {
    pub id:   String,
    pub name: String,
    #[serde(default)]
    pub query: String,
    /// Folder the search runs in. Empty → the current folder at run time.
    #[serde(default)]
    pub root: String,
    /// Recurse into subfolders.
    #[serde(default)]
    pub recursive: bool,
    /// Kind ids to keep (`image`/`document`/`video`/`audio`/`code`/`archive`/
    /// `folder`/`other`). Empty → all kinds.
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Minimum / maximum size in bytes (`None` → unbounded).
    #[serde(default)]
    pub min_bytes: Option<u64>,
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// Keep items modified at/after — or at/before — these Unix-ms timestamps.
    #[serde(default)]
    pub modified_after: Option<i64>,
    #[serde(default)]
    pub modified_before: Option<i64>,
}

// ── Persistence ────────────────────────────────────────────────────────────────

/// sitta's own config file: `arbor/profiles/<active>/sitta/config.toml`. Resolved
/// directly (not pushed by the shell) — `init_active_profile()` ran in `main`.
pub fn config_path() -> PathBuf {
    arbor_core::prelude::sitta_config_path("config.toml")
}

/// Read the sitta config. A missing / unparseable file yields defaults, never an
/// error — explorer settings are non-critical and self-heal to defaults.
pub fn load() -> SittaConfig {
    if let Ok(text) = std::fs::read_to_string(config_path()) {
        if let Ok(cfg) = toml::from_str::<SittaConfig>(&text) {
            return cfg;
        }
    }
    SittaConfig::default()
}

/// Persist the sitta config to its own file (pretty TOML), creating the dir if needed.
pub fn save(cfg: &SittaConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}
