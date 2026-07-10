//! `corvus_config` domain — the **global** corvus configuration (`corvus/config.toml`)
//! owned **out-of-process** by corvus-be.
//!
//! This module makes corvus-be the OWNER of the global corvus config file, mirroring
//! the per-repo [`crate::repo_config`] owner. The shell's `AppConfig` sub-structs are
//! replicated here **verbatim** — every serde attribute, default fn, and enum
//! preserved — so `corvus/config.toml` round-trips byte-for-byte whether the
//! read/write runs in- or out-of-process. The only difference from the shell copy is
//! the error type (`String` instead of `AppError`) and the dependency types coming
//! from `corvus_git::prelude` (`SnapshotPolicy`, `GitFlowConfig`, `StorageBackend`)
//! rather than the shell's re-exports — they are the *same* underlying types, so the
//! wire format is identical.
//!
//! The shell pushes only the profile-resolved corvus product DIRECTORY into the
//! config bag under the key `"corvus_config_dir"` (a JSON string); corvus-be
//! composes its own filenames under it (`config.toml`, `linked_worktrees.toml`).
//! corvus-be is a separate process and cannot resolve the active profile itself,
//! so it composes the file path from the dir the shell pushed.
//!
//! [`load`] is infallible-by-design: any error (path not pushed yet, file missing,
//! parse error) yields [`CorvusConfig::default`] so operational reads
//! (`crate::repo::snapshot_policy` / `diff_context_lines` / `status_detect_renames`,
//! the gitflow overlay, ticket-link defaults) never break.

use std::path::Path;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{GitFlowConfig, SnapshotPolicy};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Top-level config — replicated from the shell's `AppConfig` (the slices
// corvus owns). `recovery` reuses `SnapshotPolicy` (wire-compatible with the
// shell's `RecoveryConfig`); `gitflow` reuses `GitFlowConfig`. Both come from
// `corvus_git::prelude` — the same types the shell re-exports.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CorvusConfig {
    #[serde(default)] pub diff: DiffConfig,
    #[serde(default)] pub graph: GraphConfig,
    #[serde(default)] pub cache: CacheConfig,
    #[serde(default)] pub ticket_links: TicketLinksGlobalConfig,
    #[serde(default)] pub issues: IssuesConfig,
    #[serde(default)] pub mr: MrConfig,
    #[serde(default)] pub status: StatusConfig,
    #[serde(default)] pub recovery: corvus_git::prelude::SnapshotPolicy,
    #[serde(default)] pub missing_projects: MissingProjectsConfig,
    #[serde(default)] pub pipelines: PipelinesConfig,
    #[serde(default)] pub commit: CommitConfig,
    #[serde(default)] pub branches: BranchesConfig,
    #[serde(default)] pub gitflow: corvus_git::prelude::GitFlowConfig,
    #[serde(default)] pub studio: StudioSettings,
    #[serde(default)] pub graph_columns: GraphColumnsConfig,
    #[serde(default)] pub onboarding: OnboardingConfig,
    #[serde(default)] pub sync: SyncConfig,
}

// ---------------------------------------------------------------------------
// Sub-structs — replicated verbatim from the shell's `app_config.rs`.
// Serde attributes are load-bearing: they keep the TOML wire format identical.
// ---------------------------------------------------------------------------

// ── diff ──
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode { Unified, Split, WordDiff }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffAlgorithm { Myers, Patience, Histogram }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileListView { #[default] List, Tree }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffConfig {
    pub algorithm: DiffAlgorithm,
    pub context_lines: u32,
    pub word_wrap: bool,
    #[serde(default)] pub full_file: bool,
    #[serde(default = "default_virt_threshold")] pub virt_threshold: u32,
    #[serde(default = "default_diff_mode_split")] pub mode: DiffMode,
    #[serde(default)] pub file_list_view: FileListView,
    #[serde(default = "default_true_diff")] pub confirm_discard: bool,
    #[serde(default = "default_tab_width")] pub tab_width: u32,
}
fn default_virt_threshold() -> u32 { 200 }
fn default_diff_mode_split() -> DiffMode { DiffMode::Split }
fn default_true_diff() -> bool { true }
fn default_tab_width() -> u32 { 4 }
impl Default for DiffConfig {
    fn default() -> Self {
        Self { algorithm: DiffAlgorithm::Myers, context_lines: 3, word_wrap: false, full_file: false,
            virt_threshold: default_virt_threshold(), mode: default_diff_mode_split(),
            file_list_view: FileListView::default(), confirm_discard: default_true_diff(), tab_width: default_tab_width() }
    }
}

// ── graph ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub page_size: usize,
    pub show_remote_branches: bool,
    pub show_tags: bool,
    #[serde(default = "default_true")] pub paginate: bool,
    #[serde(default = "default_true")] pub ticket_links_enabled: bool,
}
impl Default for GraphConfig {
    fn default() -> Self { Self { page_size: 500, show_remote_branches: true, show_tags: true, paginate: true, ticket_links_enabled: true } }
}

// ── cache ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub enabled: bool,
    pub max_tabs: usize,
    pub refresh_interval_secs: u64,
    pub scheduler_enabled: bool,
    #[serde(default)] pub auto_evict_enabled: bool,
    #[serde(default = "default_tab_idle_secs")] pub tab_idle_secs: u64,
    #[serde(default = "default_evict_check_interval_secs")] pub evict_check_interval_secs: u64,
    #[serde(default = "default_true")] pub close_repo_on_evict: bool,
    #[serde(default = "default_min_cached_tabs")] pub min_cached_tabs: usize,
    #[serde(default = "default_repo_browser_ttl_secs")] pub repo_browser_ttl_secs: u64,
}
fn default_tab_idle_secs() -> u64 { 300 }
fn default_evict_check_interval_secs() -> u64 { 60 }
fn default_min_cached_tabs() -> usize { 1 }
fn default_repo_browser_ttl_secs() -> u64 { 600 }
impl Default for CacheConfig {
    fn default() -> Self {
        Self { enabled: true, max_tabs: 10, refresh_interval_secs: 60, scheduler_enabled: true,
            auto_evict_enabled: false, tab_idle_secs: default_tab_idle_secs(),
            evict_check_interval_secs: default_evict_check_interval_secs(), close_repo_on_evict: true,
            min_cached_tabs: default_min_cached_tabs(), repo_browser_ttl_secs: default_repo_browser_ttl_secs() }
    }
}

// ── ticket_links (global) ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLinksGlobalConfig {
    #[serde(default = "default_true")] pub enabled: bool,
    #[serde(default)] pub storage: corvus_git::prelude::StorageBackend,
    #[serde(default = "default_true")] pub auto_parse: bool,
    #[serde(default = "default_true")] pub warn_push: bool,
}
impl Default for TicketLinksGlobalConfig {
    fn default() -> Self { Self { enabled: true, storage: Default::default(), auto_parse: true, warn_push: true } }
}

// ── issues ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuesConfig {
    #[serde(default = "default_sort_field")] pub sort_field: String,
    #[serde(default = "default_sort_dir")] pub sort_dir: String,
}
fn default_sort_field() -> String { "updated_at".into() }
fn default_sort_dir() -> String { "desc".into() }
impl Default for IssuesConfig {
    fn default() -> Self { Self { sort_field: default_sort_field(), sort_dir: default_sort_dir() } }
}

// ── mr ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrConfig {
    #[serde(default = "default_true_mr")] pub default_show_comments: bool,
    #[serde(default = "default_true_mr")] pub default_show_bots: bool,
    #[serde(default = "default_true_mr")] pub default_show_activity: bool,
}
fn default_true_mr() -> bool { true }
impl Default for MrConfig {
    fn default() -> Self { Self { default_show_comments: true, default_show_bots: true, default_show_activity: true } }
}

// ── status ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusConfig {
    #[serde(default = "default_detect_renames")] pub detect_renames: bool,
}
fn default_detect_renames() -> bool { true }
impl Default for StatusConfig { fn default() -> Self { Self { detect_renames: true } } }

// ── missing_projects ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingProjectsConfig {
    #[serde(default)] pub auto_prune_recents: bool,
    #[serde(default = "default_true_missing")] pub confirm_before_remove: bool,
    #[serde(default = "default_true_missing")] pub revalidate_on_focus: bool,
}
fn default_true_missing() -> bool { true }
impl Default for MissingProjectsConfig {
    fn default() -> Self { Self { auto_prune_recents: false, confirm_before_remove: true, revalidate_on_focus: true } }
}

// ── pipelines ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinesConfig {
    #[serde(default = "default_max_concurrent_runs")] pub max_concurrent_runs: u32,
}
fn default_max_concurrent_runs() -> u32 { 4 }
impl Default for PipelinesConfig { fn default() -> Self { Self { max_concurrent_runs: default_max_concurrent_runs() } } }

// ── commit ──
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitConfig {
    #[serde(default)] pub template_global: String,
}

// ── branches ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchesConfig {
    #[serde(default = "default_true_branches")] pub grouping_recursive: bool,
}
fn default_true_branches() -> bool { true }
impl Default for BranchesConfig { fn default() -> Self { Self { grouping_recursive: true } } }

// ── studio ──
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StudioSettings {
    #[serde(default)] pub use_index: bool,
}

// ── graph_columns ──
// Commit-graph column layout (order, width, visibility). Migrated from the
// shell's standalone `graph_columns.toml` into the corvus product config: it's a
// git-graph concern, so corvus-be owns it like every other corvus setting. The
// special `graph` entry is the SVG lane renderer and reorders like any column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphColumnsConfig {
    #[serde(default = "default_graph_columns")] pub columns: Vec<GraphColumn>,
}
impl Default for GraphColumnsConfig {
    fn default() -> Self { Self { columns: default_graph_columns() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphColumn {
    /// Stable id: `graph`, `refs`, `subject`, `author`, `date`, `hash`.
    pub id: String,
    /// Track width in px. For `graph` it's a *max* (SVG auto-sizes + caps); for
    /// `subject` a *min* (`minmax(width, 1fr)` flex-grow); else fixed.
    pub width: u32,
    #[serde(default = "default_true")] pub visible: bool,
}
fn default_graph_columns() -> Vec<GraphColumn> {
    vec![
        GraphColumn { id: "graph".into(),   width: 480, visible: true },
        GraphColumn { id: "refs".into(),    width: 220, visible: true },
        GraphColumn { id: "subject".into(), width: 280, visible: true },
        GraphColumn { id: "author".into(),  width: 160, visible: true },
        GraphColumn { id: "date".into(),    width: 150, visible: true },
        GraphColumn { id: "hash".into(),    width:  80, visible: true },
    ]
}

// ── onboarding ──
// First-run onboarding tour state, now **per-product**: corvus owns its own
// onboarding (the welcome wizard for the git product) instead of the launcher
// shell owning a single global flag. `version` is a schema-bump knob — the
// frontend re-opens the modal when its `CURRENT_ONBOARDING_VERSION` exceeds the
// stored one. Other products (merula, …) get their own onboarding section in
// their own backend config when they grow a real first-run tour.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OnboardingConfig {
    /// User has finished or skipped the tour at least once.
    #[serde(default)] pub completed: bool,
    /// Onboarding schema version the user has been through. `0` = never seen.
    #[serde(default)] pub version: u32,
}

fn default_true() -> bool { true }

// ── sync ──
// Settings-sync to a private git-provider repo (see [`crate::sync`]). Owned by
// corvus-be like every other corvus setting; the status fields (`last_*`) are
// written back by the sync engine after a push/pull. Machine-specific and heavy
// data are intentionally out of scope — see the sync module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Master switch. When false the driver idles and no push/pull runs.
    #[serde(default)] pub enabled: bool,
    /// Provider host key (`"github"` | `"gitlab"`). Mandatory to enable.
    #[serde(default)] pub provider: Option<String>,
    /// User-chosen repo name; `None` → auto (`arbor-corvus-sync`, created private).
    #[serde(default)] pub repo_name: Option<String>,
    /// Resolved `"owner/name"` once the repo is created or adopted.
    #[serde(default)] pub repo_full_name: Option<String>,
    /// HTTPS clone URL of the resolved repo (informational / future git use).
    #[serde(default)] pub clone_url: Option<String>,
    /// Minimum seconds between auto-pushes (the debounce window).
    #[serde(default = "default_sync_interval")] pub interval_secs: u64,
    #[serde(default = "default_true")] pub include_workspaces: bool,
    #[serde(default = "default_true")] pub include_settings: bool,
    #[serde(default = "default_true")] pub include_mods: bool,
    #[serde(default = "default_true")] pub include_plugin_data: bool,
    /// Skip any per-plugin `global.json` larger than this (keeps heavy blobs out).
    #[serde(default = "default_plugin_data_cap_kb")] pub plugin_data_cap_kb: u64,
    // ── Status (written back by the engine) ──────────────────────────────────
    #[serde(default)] pub last_push_at: Option<i64>,
    #[serde(default)] pub last_pull_at: Option<i64>,
    #[serde(default)] pub last_machine: Option<String>,
}
fn default_sync_interval() -> u64 { 300 }
fn default_plugin_data_cap_kb() -> u64 { 256 }
impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enabled: false, provider: None, repo_name: None, repo_full_name: None,
            clone_url: None, interval_secs: default_sync_interval(),
            include_workspaces: true, include_settings: true, include_mods: true,
            include_plugin_data: true, plugin_data_cap_kb: default_plugin_data_cap_kb(),
            last_push_at: None, last_pull_at: None, last_machine: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence — the shell pushes only the profile-resolved corvus product
// DIRECTORY (config bag key `"corvus_config_dir"`); corvus-be composes its own
// filenames under it. corvus-be is a separate process and cannot resolve the
// active profile itself, but it owns its filenames.
// ---------------------------------------------------------------------------

/// The profile-resolved corvus product directory, pushed by the shell under the
/// config bag key `"corvus_config_dir"`. corvus-be composes its own filenames
/// under it (`config.toml`, `linked_worktrees.toml` — see [`crate::worktree_links`]).
pub(crate) fn corvus_config_dir(state: &CorvusState) -> Result<String, String> {
    state.config("corvus_config_dir")
        .and_then(|v| v.as_str().map(str::to_string))
        .ok_or_else(|| "corvus config dir not set (shell has not pushed it yet)".to_string())
}

fn config_file_path(state: &CorvusState) -> Result<String, String> {
    corvus_config_dir(state)
        .map(|dir| Path::new(&dir).join("config.toml").to_string_lossy().into_owned())
}

/// Load the owned corvus config. Infallible: any error (path not pushed yet,
/// file missing, parse error) yields defaults so operational reads never break.
pub fn load(state: &CorvusState) -> CorvusConfig {
    match config_file_path(state) {
        Ok(p) => std::fs::read_to_string(&p).ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default(),
        Err(_) => CorvusConfig::default(),
    }
}

pub(crate) fn save(state: &CorvusState, cfg: &CorvusConfig) -> Result<(), String> {
    let p = config_file_path(state)?;
    let content = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    if let Some(parent) = Path::new(&p).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, content).map_err(|e| e.to_string())?;
    // A config write is a candidate change for settings-sync; the engine's
    // fingerprint decides whether it actually differs from what was last pushed.
    crate::sync::mark_dirty();
    Ok(())
}

/// Load → mutate only the `sync` section → save. Used by the sync engine to
/// write status fields (`last_push_at`, resolved repo) without clobbering the
/// rest of the config.
pub(crate) fn update_sync(
    state: &CorvusState,
    f: impl FnOnce(&mut SyncConfig),
) -> Result<(), String> {
    let mut c = load(state);
    f(&mut c.sync);
    save(state, &c)
}

// ---------------------------------------------------------------------------
// Handlers — mirror `repo_config.rs`'s macro/signature shape. The method names
// and param names match the frontend payloads.
// ---------------------------------------------------------------------------

// ── recovery ──

#[arbor_rpc::handler]
fn get_recovery_config(state: &CorvusState) -> Result<SnapshotPolicy, String> {
    Ok(load(state).recovery)
}

#[arbor_rpc::handler]
fn set_recovery_config(state: &CorvusState, recovery: SnapshotPolicy) -> Result<(), String> {
    let mut c = load(state);
    c.recovery = recovery;
    save(state, &c)
}

// ── graph ──

#[arbor_rpc::handler]
fn get_graph_config(state: &CorvusState) -> Result<GraphConfig, String> {
    Ok(load(state).graph)
}

#[arbor_rpc::handler]
fn set_graph_config(state: &CorvusState, config: GraphConfig) -> Result<(), String> {
    let mut c = load(state);
    c.graph = config;
    save(state, &c)
}

// ── cache ──

#[arbor_rpc::handler]
fn get_cache_config(state: &CorvusState) -> Result<CacheConfig, String> {
    Ok(load(state).cache)
}

#[arbor_rpc::handler]
fn set_cache_config(state: &CorvusState, config: CacheConfig) -> Result<(), String> {
    let mut c = load(state);
    c.cache = config;
    save(state, &c)
}

// ── pipelines ──

#[arbor_rpc::handler]
fn get_pipelines_config(state: &CorvusState) -> Result<PipelinesConfig, String> {
    Ok(load(state).pipelines)
}

#[arbor_rpc::handler]
fn set_pipelines_config(state: &CorvusState, config: PipelinesConfig) -> Result<(), String> {
    let mut c = load(state);
    c.pipelines = config;
    save(state, &c)
}

// ── studio ──

#[arbor_rpc::handler]
fn get_studio_settings(state: &CorvusState) -> Result<StudioSettings, String> {
    Ok(load(state).studio)
}

#[arbor_rpc::handler]
fn set_studio_settings(state: &CorvusState, settings: StudioSettings) -> Result<(), String> {
    let mut c = load(state);
    c.studio = settings;
    save(state, &c)
}

// ── missing_projects ──

#[arbor_rpc::handler]
fn get_missing_projects_config(state: &CorvusState) -> Result<MissingProjectsConfig, String> {
    Ok(load(state).missing_projects)
}

#[arbor_rpc::handler]
fn set_missing_projects_config(state: &CorvusState, config: MissingProjectsConfig) -> Result<(), String> {
    let mut c = load(state);
    c.missing_projects = config;
    save(state, &c)
}

// ── issues ──

#[arbor_rpc::handler]
fn get_issues_config(state: &CorvusState) -> Result<IssuesConfig, String> {
    Ok(load(state).issues)
}

#[arbor_rpc::handler]
fn set_issues_config(state: &CorvusState, config: IssuesConfig) -> Result<(), String> {
    let mut c = load(state);
    c.issues = config;
    save(state, &c)
}

// ── diff ──

#[arbor_rpc::handler]
fn get_diff_config(state: &CorvusState) -> Result<DiffConfig, String> {
    Ok(load(state).diff)
}

#[arbor_rpc::handler]
fn set_diff_config(state: &CorvusState, config: DiffConfig) -> Result<(), String> {
    let mut c = load(state);
    c.diff = config;
    save(state, &c)
}

// ── mr ──

#[arbor_rpc::handler]
fn get_mr_config(state: &CorvusState) -> Result<MrConfig, String> {
    Ok(load(state).mr)
}

#[arbor_rpc::handler]
fn set_mr_config(state: &CorvusState, config: MrConfig) -> Result<(), String> {
    let mut c = load(state);
    c.mr = config;
    save(state, &c)
}

// ── branches ──

#[arbor_rpc::handler]
fn get_branches_config(state: &CorvusState) -> Result<BranchesConfig, String> {
    Ok(load(state).branches)
}

#[arbor_rpc::handler]
fn set_branches_config(state: &CorvusState, config: BranchesConfig) -> Result<(), String> {
    let mut c = load(state);
    c.branches = config;
    save(state, &c)
}

// ── commit ──

#[arbor_rpc::handler]
fn get_commit_config(state: &CorvusState) -> Result<CommitConfig, String> {
    Ok(load(state).commit)
}

#[arbor_rpc::handler]
fn set_commit_config(state: &CorvusState, config: CommitConfig) -> Result<(), String> {
    let mut c = load(state);
    c.commit = config;
    save(state, &c)
}

// ── gitflow (global) ──

#[arbor_rpc::handler]
fn get_gitflow_global_config(state: &CorvusState) -> Result<GitFlowConfig, String> {
    Ok(load(state).gitflow)
}

#[arbor_rpc::handler]
fn set_gitflow_global_config(state: &CorvusState, config: GitFlowConfig) -> Result<(), String> {
    let mut c = load(state);
    c.gitflow = config;
    save(state, &c)
}

// ── graph_columns ──

#[arbor_rpc::handler]
fn get_graph_columns(state: &CorvusState) -> Result<GraphColumnsConfig, String> {
    Ok(load(state).graph_columns)
}

#[arbor_rpc::handler]
fn set_graph_columns(state: &CorvusState, config: GraphColumnsConfig) -> Result<(), String> {
    let mut c = load(state);
    c.graph_columns = config;
    save(state, &c)
}

// ── onboarding ──

#[arbor_rpc::handler]
fn get_onboarding_config(state: &CorvusState) -> Result<OnboardingConfig, String> {
    Ok(load(state).onboarding)
}

#[arbor_rpc::handler]
fn set_onboarding_config(state: &CorvusState, config: OnboardingConfig) -> Result<(), String> {
    let mut c = load(state);
    c.onboarding = config;
    save(state, &c)
}

// ── sync ──
// Read/write the non-secret sync knobs (interval, include toggles). Enabling the
// repo itself goes through `crate::sync::handlers::sync_enable` (it must resolve
// or create the remote); this setter only tweaks an already-configured sync.

#[arbor_rpc::handler]
fn get_sync_config(state: &CorvusState) -> Result<SyncConfig, String> {
    Ok(load(state).sync)
}

#[arbor_rpc::handler]
fn set_sync_config(state: &CorvusState, config: SyncConfig) -> Result<(), String> {
    let mut c = load(state);
    c.sync = config;
    save(state, &c)
}
