use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::git::gitflow::GitFlowConfig;
use crate::git::ticket_links::StorageBackend;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Paths and extensions to exclude from repository statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsExcludeConfig {
    /// File extensions to exclude, e.g. [".ron", ".lock"].
    /// Leading dot is optional — both ".ron" and "ron" are accepted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// Folder prefixes to exclude, e.g. ["assets/generated", "vendor"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folders: Vec<String>,
    /// Exact file names or relative paths to exclude, e.g. ["Cargo.lock"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// Per-repository configuration stored in `.arbor/config.toml` inside the repo.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoConfig {
    /// Custom name override for display in tabs.
    pub display_name: Option<String>,
    /// Default remote name (falls back to "origin").
    pub default_remote: Option<String>,
    /// Branches to always show even when filtered.
    pub pinned_branches: Vec<String>,
    /// Per-repo author identity override (overrides global git config).
    #[serde(default)]
    pub user: RepoUserConfig,
    /// Per-repo Git Flow config — overrides the global AppConfig.gitflow when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitflow: Option<GitFlowConfig>,
    /// Issue tracker to use for this repository ("linear", "jira", …). None = not configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_tracker: Option<String>,
    /// Project ID to always filter issues by in the sidebar/ticket picker for this repo.
    /// The ID is provider-specific (Linear project ID or Jira project key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_tracker_project_id: Option<String>,
    /// Per-repo ticket-link overrides. When present, these shadow the global AppConfig values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket_links: Option<TicketLinksRepoConfig>,
    /// Files/folders/extensions to exclude from statistics computation.
    #[serde(default, skip_serializing_if = "stats_exclude_is_empty")]
    pub stats_exclude: StatsExcludeConfig,
    /// Tag names created locally that have not been pushed to a remote.
    /// Git itself doesn't track this distinction (a tag is just a ref) so
    /// we persist the list here to drive the "local" badge in the sidebar.
    /// Cleared when the tag is pushed or deleted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_only_tags: Vec<String>,
    /// Preferred IDE for this repository. When set, "Open in IDE" actions
    /// that don't specify a target IDE pick this one instead of the
    /// global `AppConfig.ide.default_ide`. Value is an `IdeEntry.id` or a
    /// built-in IDE id (e.g. "vscode", "intellij"). `None` ⇒ defer to
    /// the global default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ide_id: Option<String>,
}

fn stats_exclude_is_empty(e: &StatsExcludeConfig) -> bool {
    e.extensions.is_empty() && e.folders.is_empty() && e.files.is_empty()
}

/// Per-repository overrides for ticket-link behaviour.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TicketLinksRepoConfig {
    /// Override the storage backend for this repo only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage: Option<StorageBackend>,
    /// Active issue tracker for this repo ("linear", "jira", "github", "gitlab").
    /// Falls back to `RepoConfig::issue_tracker` when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracker: Option<String>,
    /// Override auto-parse for this repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_parse: Option<bool>,
    /// Custom regex pattern for ticket ID extraction (overrides the tracker default).
    /// Must contain exactly one capture group, e.g. `"\\b(MYCO-\\d+)\\b"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_pattern: Option<String>,
}

/// Author/identity override for a specific repository.
/// When set, Arbor uses these values instead of the global git config
/// for commits made in this repository.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RepoUserConfig {
    /// Override commit author name (None = use global git config).
    pub name: Option<String>,
    /// Override commit author email (None = use global git config).
    pub email: Option<String>,
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn repo_config_path(repo_path: &str) -> PathBuf {
    PathBuf::from(repo_path).join(".arbor").join("config.toml")
}

pub fn load(repo_path: &str) -> Result<RepoConfig> {
    let path = repo_config_path(repo_path);
    if !path.exists() {
        return Ok(RepoConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: RepoConfig = toml::from_str(&content)?;
    Ok(config)
}

pub fn save(repo_path: &str, config: &RepoConfig) -> Result<()> {
    let path = repo_config_path(repo_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content).map_err(AppError::Io)
}
