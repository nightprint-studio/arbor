//! `repo_config` domain — per-repository configuration (`<repo>/.arbor/config.toml`)
//! served **out-of-process** by corvus-be.
//!
//! The shell's [`RepoConfig`] (and its sub-structs) is replicated here **verbatim**
//! — every serde attribute, default fn, and skip-predicate preserved — so the
//! `.arbor/config.toml` round-trips byte-for-byte whether the read/write runs in-
//! or out-of-process. The only difference from the shell copy is the error type
//! (`String` instead of `AppError`) and the dependency types coming from
//! `corvus_git::prelude` (`GitFlowConfig`, `StorageBackend`) rather than the shell's
//! re-exports — they are the *same* underlying types, so the wire format is identical.
//!
//! Handlers resolve a `tab_id` to the repo workdir via [`crate::repo::repo_path`]
//! (the shell pushes the workdir on repo open; `.arbor/` lives next to it — the same
//! direct-read precedent the `stats` / `gitflow` / `tickets` domains use). Behaviour
//! (field mutations, early-returns, conditional-save logic) is byte-identical to the
//! shell's in-process copies in `crate::ipc::platform::config`,
//! `crate::ipc::corvus::ide`, and `crate::ipc::corvus::gitflow`.
//!
//! `get_gitflow_config` resolves the **effective** config: the global gitflow the
//! shell pushed into the config bag (section `"gitflow"`, read via
//! [`CorvusState::config`]) overlaid by the repo's own `.arbor/config.toml` override
//! when present — identical resolution to the shell's `effective_config`.
//!
//! The per-repo ticket-link write (`set_ticket_link_repo_config`) is **not** here —
//! it already lives in the `tickets` domain (`crate::tickets`), which merges only the
//! `[ticket_links]` table to preserve its cache invalidation. No hooks fire in this
//! domain.

use std::path::PathBuf;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{GitFlowConfig, StorageBackend};
use serde::{Deserialize, Serialize};

use crate::repo::repo_path;

// ---------------------------------------------------------------------------
// Types — replicated verbatim from the shell's `crate::config::repo_config`.
// Serde attributes are load-bearing: they keep the TOML wire format identical.
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
    /// Local & remote branch list grouping (folder-tree by `/` segments).
    /// On by default — almost every real-world repo has slash-prefixed
    /// branches and a flat 20+ entry list is harder to read than the
    /// folded one. Users who prefer flat flip it from the sidebar toggle
    /// or the `toggle_branch_grouping` keybinding.
    #[serde(default, skip_serializing_if = "branch_grouping_is_default")]
    pub branch_grouping: BranchGroupingConfig,
}

/// Per-repo branch grouping state (folder-tree view of `feature/x` paths).
///
/// `collapsed_groups` lists the group paths the user has explicitly
/// collapsed (joined with `/`, e.g. `feature` or `feature/auth`). Anything
/// not in the list renders expanded, so first-time grouping pops the full
/// tree open instead of an empty stack the user has to reveal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchGroupingConfig {
    #[serde(default = "default_true_branch_grouping")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collapsed_groups: Vec<String>,
}

fn default_true_branch_grouping() -> bool { true }

impl Default for BranchGroupingConfig {
    fn default() -> Self {
        Self { enabled: true, collapsed_groups: Vec::new() }
    }
}

fn branch_grouping_is_default(g: &BranchGroupingConfig) -> bool {
    g.enabled && g.collapsed_groups.is_empty()
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
// Persistence — replicated from the shell, error type `String` instead of
// `AppError` (the corvus-be handler error surface).
// ---------------------------------------------------------------------------

pub fn repo_config_path(repo_path: &str) -> PathBuf {
    PathBuf::from(repo_path).join(".arbor").join("config.toml")
}

pub fn load(repo_path: &str) -> Result<RepoConfig, String> {
    let path = repo_config_path(repo_path);
    if !path.exists() {
        return Ok(RepoConfig::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let config: RepoConfig = toml::from_str(&content).map_err(|e| e.to_string())?;
    Ok(config)
}

pub fn save(repo_path: &str, config: &RepoConfig) -> Result<(), String> {
    let path = repo_config_path(repo_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// RepoConfig get / set
// ---------------------------------------------------------------------------

/// Load per-repository configuration from `.arbor/config.toml` inside the repo.
#[arbor_rpc::handler]
fn get_repo_config(state: &CorvusState, tab_id: String) -> Result<RepoConfig, String> {
    let workdir = repo_path(state, &tab_id)?;
    load(&workdir)
}

/// Persist per-repository configuration to `.arbor/config.toml`.
#[arbor_rpc::handler]
fn set_repo_config(state: &CorvusState, tab_id: String, config: RepoConfig) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    save(&workdir, &config)
}

// ---------------------------------------------------------------------------
// Local-only tag tracking
//
// Git has no built-in concept of "tag not yet pushed", so we persist a list of
// locally-created tag names in `.arbor/config.toml`. Removed when the tag is
// pushed (or deleted).
// ---------------------------------------------------------------------------

/// Return the list of tag names flagged as local-only for this repo.
#[arbor_rpc::handler]
fn list_local_only_tags(state: &CorvusState, tab_id: String) -> Result<Vec<String>, String> {
    let workdir = repo_path(state, &tab_id)?;
    Ok(load(&workdir)?.local_only_tags)
}

/// Mark a tag as locally-created and not-yet-pushed.
#[arbor_rpc::handler]
fn mark_tag_local(state: &CorvusState, tab_id: String, name: String) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut config = load(&workdir)?;
    if !config.local_only_tags.iter().any(|n| n == &name) {
        config.local_only_tags.push(name);
        save(&workdir, &config)?;
    }
    Ok(())
}

/// Mark a tag as pushed (or deleted) — removes it from the local-only list.
#[arbor_rpc::handler]
fn mark_tag_pushed(state: &CorvusState, tab_id: String, name: String) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut config = load(&workdir)?;
    let before = config.local_only_tags.len();
    config.local_only_tags.retain(|n| n != &name);
    if config.local_only_tags.len() != before {
        save(&workdir, &config)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Branch grouping (per-repo state — convenience wrapper over RepoConfig)
// ---------------------------------------------------------------------------

/// Read the per-repo branch-grouping state (enabled flag + collapsed groups).
/// Convenience wrapper over `RepoConfig.branch_grouping` so the frontend store
/// doesn't have to round-trip the entire RepoConfig on every toggle.
#[arbor_rpc::handler]
fn get_branch_grouping(state: &CorvusState, tab_id: String) -> Result<BranchGroupingConfig, String> {
    let workdir = repo_path(state, &tab_id)?;
    Ok(load(&workdir)?.branch_grouping)
}

/// Persist per-repo branch-grouping state (enabled + collapsed groups).
#[arbor_rpc::handler]
fn set_branch_grouping(
    state: &CorvusState,
    tab_id: String,
    config: BranchGroupingConfig,
) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut cfg = load(&workdir)?;
    cfg.branch_grouping = config;
    save(&workdir, &cfg)
}

// ---------------------------------------------------------------------------
// Per-repo IDE preference (`.arbor/config.toml` → `ide_id`)
// ---------------------------------------------------------------------------

/// Read the project-bound IDE preference, or `None` when the repo defers to the
/// global default. Convenience wrapper over `get_repo_config` so the Settings
/// panel doesn't have to round-trip the whole RepoConfig.
#[arbor_rpc::handler]
fn get_repo_ide(state: &CorvusState, tab_id: String) -> Result<Option<String>, String> {
    let workdir = repo_path(state, &tab_id)?;
    Ok(load(&workdir)?.ide_id)
}

/// Persist (or clear) the project-bound IDE preference. Pass `None` to remove the
/// override and fall back to the global default.
#[arbor_rpc::handler]
fn set_repo_ide(state: &CorvusState, tab_id: String, ide_id: Option<String>) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut cfg = load(&workdir).unwrap_or_default();
    cfg.ide_id = ide_id.filter(|s| !s.is_empty());
    save(&workdir, &cfg)
}

// ---------------------------------------------------------------------------
// Per-repo Git Flow config (the config-CRUD slice that owns `.arbor/config.toml`)
//
// The GLOBAL gitflow config (`get`/`set_gitflow_global_config`) stays shell-owned
// (it lives in AppConfig). Here we own only the per-repo override in
// `.arbor/config.toml`, plus the effective-config merge read.
// ---------------------------------------------------------------------------

/// The effective Git Flow config for a tab: the global config the shell pushed
/// (section `"gitflow"`) overlaid by the repo's own `.arbor/config.toml` override
/// when present. Resolution is byte-identical to the in-process `effective_config`.
#[arbor_rpc::handler]
fn get_gitflow_config(state: &CorvusState, tab_id: String) -> Result<GitFlowConfig, String> {
    let global: GitFlowConfig = state
        .config("gitflow")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    let workdir = repo_path(state, &tab_id)?;
    let repo_cfg = load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.unwrap_or(global))
}

/// Persist the per-repo Git Flow override into `.arbor/config.toml`.
#[arbor_rpc::handler]
fn set_gitflow_repo_config(
    state: &CorvusState,
    tab_id: String,
    config: GitFlowConfig,
) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut repo_cfg = load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = Some(config);
    save(&workdir, &repo_cfg)
}

/// Clear the per-repo Git Flow override (fall back to the global config).
#[arbor_rpc::handler]
fn clear_gitflow_repo_config(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let workdir = repo_path(state, &tab_id)?;
    let mut repo_cfg = load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = None;
    save(&workdir, &repo_cfg)
}

/// `true` if the repo has its own Git Flow override in `.arbor/config.toml`.
#[arbor_rpc::handler]
fn has_gitflow_repo_override(state: &CorvusState, tab_id: String) -> Result<bool, String> {
    let workdir = repo_path(state, &tab_id)?;
    let repo_cfg = load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.is_some())
}
