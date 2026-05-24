use std::path::PathBuf;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::git::gitflow::GitFlowConfig;
use crate::git::ticket_links::StorageBackend;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub theme: ThemeConfig,
    pub diff: DiffConfig,
    pub graph: GraphConfig,
    pub keybindings: KeybindingsConfig,
    /// Paths of recently opened repositories.
    pub recent_repos: Vec<String>,
    /// Global Git Flow configuration (can be overridden per-repo in .arbor/config.toml).
    #[serde(default)]
    pub gitflow: GitFlowConfig,
    /// Per-tab data cache settings.
    #[serde(default)]
    pub cache: CacheConfig,
    /// Global ticket-link settings (can be overridden per-repo).
    #[serde(default)]
    pub ticket_links: TicketLinksGlobalConfig,
    /// Issue sidebar / picker display preferences.
    #[serde(default)]
    pub issues: IssuesConfig,
    /// Default visibility of the Activity-timeline filters in the MR/PR
    /// detail modal. Matches the three filter chips (Comments / Bots /
    /// Activity) — each chip is initialised from this config when a modal
    /// opens, and toggling chips inside the modal is session-only.
    #[serde(default)]
    pub mr: MrConfig,
    /// IDE launcher preferences (for "Open in IDE" from worktrees).
    #[serde(default)]
    pub ide: IdeConfig,
    /// Built-in shell + custom-terminal preferences for the integrated
    /// terminal panel.
    #[serde(default)]
    pub terminals: TerminalsConfig,
    /// Activity bar visibility and ordering.
    #[serde(default)]
    pub activity_bar: ActivityBarConfig,
    /// Status-computation tuning.  Users with very large working copies can
    /// disable rename/copy detection here to cut status-scan time from seconds
    /// to milliseconds (rename detection is O(n²) in libgit2).
    #[serde(default)]
    pub status: StatusConfig,
    /// Safety-net recovery snapshot policy.  Controls which files get their
    /// content preserved vs. only logged (by size / extension).
    #[serde(default)]
    pub recovery: RecoveryConfig,
    /// Behaviour when a registered project's path is no longer available
    /// on disk (deleted, moved, drive offline).
    #[serde(default)]
    pub missing_projects: MissingProjectsConfig,
    /// Override for the `git` executable used by Arbor's CLI shell-outs
    /// (rebase, stash, submodules, recovery snapshots, …). When empty Arbor
    /// auto-detects via PATH then the bundled portable copy.
    #[serde(default)]
    pub git: GitCliConfig,
    /// Master switch for the plugin system. When false (the default), the
    /// app starts WITHOUT loading any plugin: the runtime stays empty,
    /// schedulers don't fire, and the Plugin Manager refuses to list
    /// anything. The user must explicitly opt in via the Plugin Manager
    /// toggle. Persisted in `config.toml` so the choice survives restarts.
    #[serde(default)]
    pub plugins_enabled: bool,
    /// User-supplied OAuth client IDs that override the bundled defaults.
    /// Useful when a fork is published, a corporate proxy requires a
    /// captive client, or a self-hosted GitLab instance issues its own
    /// OAuth applications. The `client_id` is a public OAuth identifier
    /// (RFC 6749 §2.2) and is safe to store in plain TOML.
    #[serde(default)]
    pub oauth: OAuthOverrides,
    /// Pipeline orchestration tuning (concurrency cap, …).
    #[serde(default)]
    pub pipelines: PipelinesConfig,
    /// `arbor://…` deep-link routing preferences.
    #[serde(default)]
    pub deep_link: crate::deep_link::DeepLinkConfig,
    /// RON Studio settings — persistent project-wide cross-ref index +
    /// related tuning. Distinct from the per-repo `.ron-studio.toml`
    /// (`crate::studio::config::StudioConfig`) which lives next to the
    /// code itself; this struct holds host-wide tunables.
    #[serde(default)]
    pub studio: StudioSettings,
    /// Marketplace catalog auto-refresh policy.
    #[serde(default)]
    pub marketplace: MarketplaceConfig,
    /// Visual appearance preferences (window control style, …).
    #[serde(default)]
    pub appearance: AppearanceConfig,
    /// UI animation preferences (enable/disable, speed multiplier).
    #[serde(default)]
    pub animations: AnimationsConfig,
    /// Host-wide commit preferences (global template fallback, …).
    #[serde(default)]
    pub commit: CommitConfig,
    /// First-run onboarding tour state. Tracks whether the welcome wizard
    /// has been completed/dismissed and the schema version so future
    /// additions can re-prompt only for new steps.
    #[serde(default)]
    pub onboarding: OnboardingConfig,
}

/// User-facing visual tweaks. Theme lives in its own slot (the active theme id
/// is persisted via `ThemeConfig`); the smaller switches that don't belong
/// anywhere else are gathered here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppearanceConfig {
    /// Style of the close/minimize/maximize buttons in the title bar.
    /// `"mac"` (default) — the rounded coloured trio drawn at the right of
    /// the title bar; `"windows"` — flat rectangular controls in the same
    /// position and dimensions. Position is intentionally fixed regardless
    /// of style.
    #[serde(default = "default_window_controls_style")]
    pub window_controls_style: String,
    /// Global UI font scale multiplier applied to `--font-scale`.
    /// Clamped to `[0.8, 1.4]` at read time on the frontend; persisted as-is.
    #[serde(default = "default_font_scale")]
    pub font_scale: f32,
    /// When `true`, the active theme's optional `--theme-font-*` variables
    /// override the global font stack. Off by default — themes usually
    /// shouldn't be allowed to change the user's preferred font.
    #[serde(default)]
    pub use_theme_fonts: bool,
    /// Position of the built-in activity bar: left (default), right (mirror
    /// layout), or hidden (collapsed, revealed by hovering the left edge).
    #[serde(default)]
    pub activity_bar_position: ActivityBarPosition,
    /// When `true`, the title bar uses a reduced height and tighter padding.
    /// Useful on laptops where vertical space is at a premium.
    #[serde(default)]
    pub compact_title_bar: bool,
}

fn default_window_controls_style() -> String { "mac".into() }
fn default_font_scale() -> f32 { 1.0 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActivityBarPosition {
    #[default]
    Left,
    Right,
    Hidden,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            window_controls_style: default_window_controls_style(),
            font_scale:            default_font_scale(),
            use_theme_fonts:       false,
            activity_bar_position: ActivityBarPosition::default(),
            compact_title_bar:     false,
        }
    }
}

/// UI animation preferences. `enabled = false` collapses every transition
/// duration to zero so power users on remote desktops / Hyper-V can skip
/// the visual cost without losing functionality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationsConfig {
    #[serde(default = "default_true_anim")]
    pub enabled: bool,
    #[serde(default)]
    pub speed: AnimSpeed,
}

fn default_true_anim() -> bool { true }

impl Default for AnimationsConfig {
    fn default() -> Self {
        Self { enabled: true, speed: AnimSpeed::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnimSpeed {
    Fast,
    #[default]
    Normal,
    Slow,
}

/// First-run onboarding tour state. Persisted in `~/.config/arbor/config.toml`
/// under `[onboarding]` so the welcome modal only auto-pops once.
///
/// `version` is a schema bump knob: when we add meaningful new steps in a
/// future release we increment `CURRENT_ONBOARDING_VERSION` on the
/// frontend, and the modal re-opens automatically for users whose stored
/// `version` is lower (showing only the new steps, not the whole tour
/// again).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingConfig {
    /// User has finished or skipped the tour at least once.
    #[serde(default)]
    pub completed: bool,
    /// Onboarding schema the user has been through. `0` means never seen.
    #[serde(default)]
    pub version: u32,
}

impl Default for OnboardingConfig {
    fn default() -> Self {
        Self { completed: false, version: 0 }
    }
}

/// Global commit-related preferences. Per-repo overrides live in
/// `.arbor/config.toml`; this struct holds the host-wide fallbacks.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitConfig {
    /// Default commit-message template used as a fallback when the repo
    /// has no native `commit.template` configured. Empty string disables
    /// the template entirely.
    #[serde(default)]
    pub template_global: String,
}

/// Marketplace catalog auto-refresh settings.
///
/// Arbor is designed to stay open for long sessions, so a one-time fetch
/// on modal open isn't enough — the user might never open the modal but
/// still want a fresh catalog when they do. The auto-refresh scheduler
/// polls the cache age and re-fetches in the background.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceConfig {
    /// Auto-refresh interval in hours. `None` (or `0`) disables the
    /// background refresh entirely — the user has to hit the Refresh
    /// button manually. Default = 24h.
    #[serde(default = "default_refresh_hours")]
    pub refresh_hours: Option<u32>,
    /// How often the scheduler wakes up to check whether a refresh is due,
    /// in minutes. Tunable so users who set a short `refresh_hours` can
    /// also get finer wake-up granularity, while users who set 7d can let
    /// the task sleep for an hour at a time. Clamped to [1, 60] at read.
    /// Default = 10min.
    #[serde(default = "default_poll_minutes")]
    pub poll_minutes: u32,
}

fn default_refresh_hours() -> Option<u32> { Some(24) }
fn default_poll_minutes() -> u32 { 10 }

impl Default for MarketplaceConfig {
    fn default() -> Self {
        Self {
            refresh_hours: default_refresh_hours(),
            poll_minutes:  default_poll_minutes(),
        }
    }
}

/// Studio (RON / JSON / TOML sidebar) settings.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StudioSettings {
    /// When `true`, the host maintains a persistent on-disk index of
    /// every `.ron` file's top-level definitions and reference fields.
    /// Cross-ref and find-usages queries then read from the cache
    /// instead of re-walking the repo on every call. Off by default —
    /// the index is built lazily by a background job the first time it
    /// reads `true` and refreshed on each Save.
    #[serde(default)]
    pub use_index: bool,
}

/// Pipeline orchestrator settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelinesConfig {
    /// Maximum number of pipeline runs that may be `Running` at the same
    /// time across all plugins. Additional runs stay `Pending` (with
    /// `queued = true`) and start as soon as a slot frees up. `0` means
    /// unlimited — the orchestrator never queues. Default: 4.
    #[serde(default = "default_max_concurrent_runs")]
    pub max_concurrent_runs: u32,
}

fn default_max_concurrent_runs() -> u32 { 4 }

impl Default for PipelinesConfig {
    fn default() -> Self {
        Self { max_concurrent_runs: default_max_concurrent_runs() }
    }
}

// ---------------------------------------------------------------------------
// OAuth overrides
// ---------------------------------------------------------------------------

/// Per-provider OAuth `client_id` (and host, for GitLab) overrides.
/// Empty strings are treated as "use the bundled default".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OAuthOverrides {
    #[serde(default)]
    pub github: ProviderOverride,
    #[serde(default)]
    pub gitlab: GitlabOverride,
    #[serde(default)]
    pub linear: ProviderOverride,
    #[serde(default)]
    pub jira:   ProviderOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderOverride {
    #[serde(default)]
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitlabOverride {
    #[serde(default)]
    pub client_id: Option<String>,
    /// Base host for self-hosted GitLab — e.g. `gitlab.company.com`.
    /// When set, OAuth endpoints become `https://{base_host}/oauth/...`
    /// instead of the default `gitlab.com`.
    #[serde(default)]
    pub base_host: Option<String>,
}

impl OAuthOverrides {
    /// Read overrides directly from disk.  OAuth flows are user-action
    /// triggered (button click), so the disk hit is acceptable and avoids
    /// having to thread `AppState` through the auth modules.
    pub fn load_from_disk() -> Self {
        load().map(|c| c.oauth).unwrap_or_default()
    }
}

/// Apply a function to the persisted `OAuthOverrides` and save back to disk.
/// Used by the `set_oauth_*` Tauri commands.
#[allow(dead_code)]
pub fn update_oauth(mutator: impl FnOnce(&mut OAuthOverrides)) -> Result<()> {
    let mut cfg = load().unwrap_or_default();
    mutator(&mut cfg.oauth);
    save(&cfg)
}

/// Persistent config for the system git executable.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitCliConfig {
    /// Absolute path to the `git` binary.  When `None`/empty, Arbor falls
    /// back to PATH lookup, then the portable copy at `~/.config/arbor/git/`.
    #[serde(default)]
    pub executable_path: Option<String>,
}

/// Tombstone-and-locate behaviour for repositories whose path is missing.
/// All defaults are non-destructive: the tab is shown in tombstone state,
/// the user explicitly chooses what to do with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingProjectsConfig {
    /// When true, recent-repo entries that fail validation at WelcomeScreen
    /// load time are removed from disk silently.  Default false: the user
    /// sees them with a "missing" badge and decides per-entry.
    #[serde(default)]
    pub auto_prune_recents: bool,
    /// Show a confirmation dialog before deregistering a repo from the
    /// tombstone UI (vs. removing immediately on click).  Default true.
    #[serde(default = "default_true_missing")]
    pub confirm_before_remove: bool,
    /// Re-classify a tombstoned tab when the window regains focus.  Useful
    /// when the user just remounted a drive or reconnected to a VPN —
    /// avoids requiring a manual Retry click.  Default true.
    #[serde(default = "default_true_missing")]
    pub revalidate_on_focus: bool,
}

fn default_true_missing() -> bool { true }

impl Default for MissingProjectsConfig {
    fn default() -> Self {
        Self {
            auto_prune_recents:    false,
            confirm_before_remove: true,
            revalidate_on_focus:   true,
        }
    }
}

/// Persistent configuration for the recovery-snapshot policy.
/// Mirrors [`crate::git::recovery::SnapshotPolicy`] and is stored in
/// `~/.config/arbor/config.toml` under `[recovery]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Per-file size cap (bytes).  Files above this are only logged, not
    /// preserved, so the snapshot store stays bounded.
    #[serde(default = "default_recovery_max_size")]
    pub max_file_size: u64,
    /// Lower-case extensions (no leading dot) that are never preserved even
    /// if below `max_file_size`.  Intended for binary formats and build
    /// artifacts where a restore would rarely make sense.
    #[serde(default = "default_recovery_deny_exts")]
    pub deny_extensions: Vec<String>,
    /// How many days of snapshots to keep.  Entries older than this are pruned
    /// (ref deleted + journal line removed) the next time the panel is opened.
    /// Set to 0 to disable time-based expiry entirely — the entry-count cap
    /// still bounds growth in that case.
    #[serde(default = "default_recovery_retention_days")]
    pub retention_days: u32,
}

fn default_recovery_max_size() -> u64 {
    crate::git::recovery::DEFAULT_MAX_FILE_SIZE
}

fn default_recovery_deny_exts() -> Vec<String> {
    crate::git::recovery::DEFAULT_DENY_EXTENSIONS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn default_recovery_retention_days() -> u32 {
    crate::git::recovery::DEFAULT_RETENTION_DAYS
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_file_size:   default_recovery_max_size(),
            deny_extensions: default_recovery_deny_exts(),
            retention_days:  default_recovery_retention_days(),
        }
    }
}

impl From<RecoveryConfig> for crate::git::recovery::SnapshotPolicy {
    fn from(cfg: RecoveryConfig) -> Self {
        Self {
            max_file_size:   cfg.max_file_size,
            deny_extensions: cfg.deny_extensions,
            retention_days:  cfg.retention_days,
        }
    }
}

/// Runtime tuning for workdir status scans.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusConfig {
    /// When true (default), libgit2 detects renames/copies between HEAD→index
    /// and index→workdir.  On repos with thousands of modified files this is
    /// the dominant cost of a status refresh; turning it off trades rename
    /// grouping in the UI for a significant speedup.
    #[serde(default = "default_detect_renames")]
    pub detect_renames: bool,
}

fn default_detect_renames() -> bool { true }

impl Default for StatusConfig {
    fn default() -> Self {
        Self { detect_renames: true }
    }
}

// ---------------------------------------------------------------------------
// IDE configuration
// ---------------------------------------------------------------------------

/// A custom IDE entry added by the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeEntry {
    /// Unique identifier (used as the `ide_id` key).
    pub id: String,
    /// Human-readable label shown in the UI.
    pub name: String,
    /// Executable name or full path (e.g. "code", "/usr/local/bin/idea").
    pub command: String,
    /// Extra arguments passed before the path (e.g. ["--new-window"]).
    #[serde(default)]
    pub args: Vec<String>,
}

/// Global IDE launcher settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdeConfig {
    /// ID of the default IDE (built-in key or custom `IdeEntry.id`).
    #[serde(default = "default_ide_id")]
    pub default_ide: String,
    /// User-defined IDE entries that extend the built-in list.
    #[serde(default)]
    pub custom_ides: Vec<IdeEntry>,
    /// Custom executable paths for built-in IDEs.
    /// Key = ide_id (e.g. "vscode"), value = absolute path to the executable.
    /// When set, overrides the default command lookup in PATH.
    #[serde(default)]
    pub path_overrides: std::collections::HashMap<String, String>,
    /// Per-language-type IDE override.
    /// Key = project type string (e.g. "rust", "node_js"), value = ide_id.
    /// Takes precedence over `default_ide` when opening a worktree.
    #[serde(default)]
    pub language_defaults: std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Activity bar configuration
// ---------------------------------------------------------------------------

/// Visibility and position of a single activity-bar item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityBarItemConfig {
    /// Unique identifier for the item (e.g. "branches", "stats", "plugin:compile-action:run").
    pub id: String,
    /// Whether the item is shown in the activity bar.
    pub visible: bool,
}

/// Persisted activity-bar layout (order + visibility).
/// When empty the bar uses built-in defaults with all items visible.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityBarConfig {
    /// Ordered item list for the top section of the LEFT bar (sidebar toggles).
    #[serde(default)]
    pub top_items: Vec<ActivityBarItemConfig>,
    /// Ordered item list for the bottom section of the LEFT bar (panel
    /// toggles + plugin actions).
    #[serde(default)]
    pub bottom_items: Vec<ActivityBarItemConfig>,
    /// Ordered item list for the top section of the RIGHT bar.
    /// Empty by default; plugins that target `side="right"` auto-appear.
    #[serde(default)]
    pub right_top_items: Vec<ActivityBarItemConfig>,
    /// Ordered item list for the bottom section of the RIGHT bar.
    #[serde(default)]
    pub right_bottom_items: Vec<ActivityBarItemConfig>,
}

fn default_ide_id() -> String { "vscode".into() }

impl Default for IdeConfig {
    fn default() -> Self {
        Self {
            default_ide: default_ide_id(),
            custom_ides: Vec::new(),
            path_overrides: std::collections::HashMap::new(),
            language_defaults: std::collections::HashMap::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Terminal configuration
// ---------------------------------------------------------------------------

/// A custom terminal entry added by the user (free-form: any executable +
/// args combo). Mirrors `IdeEntry`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalEntry {
    pub id:      String,
    pub name:    String,
    pub command: String,
    #[serde(default)]
    pub args:    Vec<String>,
}

/// Global terminal preferences.  Stored under `[terminals]` in the app config.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalsConfig {
    /// ID of the shell opened by the bare "+" button (built-in id or custom
    /// id).  None ⇒ platform default.
    #[serde(default)]
    pub default_shell:  Option<String>,
    /// User-defined custom terminals — always shown in the picker.
    #[serde(default)]
    pub custom_shells:  Vec<TerminalEntry>,
    /// Custom executable paths for built-in shells (id → absolute path).
    #[serde(default)]
    pub path_overrides: std::collections::HashMap<String, String>,
}

/// Issue sidebar / picker display preferences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuesConfig {
    /// Field to sort the issue list by.
    #[serde(default = "default_sort_field")]
    pub sort_field: String,
    /// Sort direction: "asc" or "desc".
    #[serde(default = "default_sort_dir")]
    pub sort_dir: String,
}

fn default_sort_field() -> String { "updated_at".into() }
fn default_sort_dir()   -> String { "desc".into() }

impl Default for IssuesConfig {
    fn default() -> Self {
        Self { sort_field: default_sort_field(), sort_dir: default_sort_dir() }
    }
}

/// Defaults for the MR/PR detail modal's Activity-timeline filter chips.
/// Each flag controls whether its category starts visible — users can flip
/// chips inside a modal at any time, but those toggles are not persisted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrConfig {
    /// Show human comments by default. Default: true.
    #[serde(default = "default_true_mr")]
    pub default_show_comments: bool,
    /// Show bot/automated comments by default. Default: true (the user
    /// expressed preference for surfacing bots so security-policy / CI
    /// notes aren't silently hidden).
    #[serde(default = "default_true_mr")]
    pub default_show_bots: bool,
    /// Show system events (state changes, label edits, …) by default.
    /// Default: true.
    #[serde(default = "default_true_mr")]
    pub default_show_activity: bool,
}

fn default_true_mr() -> bool { true }

impl Default for MrConfig {
    fn default() -> Self {
        Self {
            default_show_comments: true,
            default_show_bots:     true,
            default_show_activity: true,
        }
    }
}

/// Global defaults for the commit ↔ ticket association feature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLinksGlobalConfig {
    /// Master switch — when false, no link queries are performed.
    #[serde(default = "default_true")]
    pub enabled:    bool,
    /// Default storage backend (git_notes | links_toml).
    #[serde(default)]
    pub storage:    StorageBackend,
    /// Auto-parse commit messages / branch names for ticket IDs.
    #[serde(default = "default_true")]
    pub auto_parse: bool,
    /// Warn after push when git-notes push refspec is not configured.
    #[serde(default = "default_true")]
    pub warn_push:  bool,
}

fn default_true() -> bool { true }

impl Default for TicketLinksGlobalConfig {
    fn default() -> Self {
        Self { enabled: true, storage: StorageBackend::default(), auto_parse: true, warn_push: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    /// ID of the active theme (e.g. "dark", "light", or a custom theme id).
    /// The legacy `name` key is accepted as an alias when reading old configs.
    #[serde(alias = "name")]
    pub active: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffMode {
    Unified,
    Split,
    WordDiff,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffConfig {
    pub algorithm: DiffAlgorithm,
    pub context_lines: u32,
    pub word_wrap: bool,
    /// Render the entire file as context (one giant hunk per file) instead
    /// of the default N-context-lines slice. Defaults to false.
    #[serde(default)]
    pub full_file: bool,
    /// When a diff has more lines than this, the frontend switches to a
    /// virtualized renderer. Defaults to 200.
    #[serde(default = "default_virt_threshold")]
    pub virt_threshold: u32,
    /// Layout used by DiffViewer: split (side-by-side) vs unified (single
    /// column). `word_diff` is reserved.
    #[serde(default = "default_diff_mode_split")]
    pub mode: DiffMode,
    /// Layout used by FileDiffList: flat list vs collapsible folder tree.
    #[serde(default)]
    pub file_list_view: FileListView,
    /// Show a confirmation dialog before discarding workdir changes.
    #[serde(default = "default_true_diff")]
    pub confirm_discard: bool,
    /// Visual tab width used when rendering diff lines containing `\t`.
    /// Clamped to `[1, 16]` at read time on the frontend; persisted as-is.
    #[serde(default = "default_tab_width")]
    pub tab_width: u32,
}

fn default_virt_threshold() -> u32 { 200 }
fn default_diff_mode_split() -> DiffMode { DiffMode::Split }
fn default_true_diff() -> bool { true }
fn default_tab_width() -> u32 { 4 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileListView {
    #[default]
    List,
    Tree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffAlgorithm {
    Myers,
    Patience,
    Histogram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphConfig {
    pub page_size: usize,
    pub show_remote_branches: bool,
    pub show_tags: bool,
    /// When false the entire history is loaded at once (no lazy-load on scroll).
    #[serde(default = "default_true")]
    pub paginate: bool,
    /// Render the ticket-link chip column in the commit graph. Independent
    /// of the per-repo ticket-link feature toggle (which gates fetches): when
    /// `false` the chips are hidden even if links are available.
    #[serde(default = "default_true")]
    pub ticket_links_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeybindingsConfig {
    pub bindings: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable the per-tab data cache.
    pub enabled: bool,
    /// Maximum number of tabs whose data is kept in the LRU cache simultaneously.
    pub max_tabs: usize,
    /// How often the active tab is checked for remote changes (seconds).
    pub refresh_interval_secs: u64,
    /// Enable the background auto-refresh scheduler.
    pub scheduler_enabled: bool,
    /// Enable automatic eviction of idle tab caches.
    #[serde(default)]
    pub auto_evict_enabled: bool,
    /// Seconds a tab must be idle before its backend cache entries are evicted.
    #[serde(default = "default_tab_idle_secs")]
    pub tab_idle_secs: u64,
    /// How often the eviction scheduler checks for idle tabs (seconds).
    #[serde(default = "default_evict_check_interval_secs")]
    pub evict_check_interval_secs: u64,
    /// When evicting a tab's cache, also drop the git2::Repository handle to
    /// free libgit2 internal caches (pack indexes, ref cache, ODB).
    /// The repo is transparently re-opened on next access.
    #[serde(default = "default_true")]
    pub close_repo_on_evict: bool,
    /// Minimum number of most-recently-used tabs to always keep in cache,
    /// regardless of idle time. The active tab counts toward this total.
    #[serde(default = "default_min_cached_tabs")]
    pub min_cached_tabs: usize,
    /// TTL (seconds) for the Repository Browser repo list cache.  GitHub /
    /// GitLab "list all repos for user" is slow on large accounts (200+
    /// projects) so the frontend caches results until this TTL expires.
    /// Zero = disabled.
    #[serde(default = "default_repo_browser_ttl_secs")]
    pub repo_browser_ttl_secs: u64,
}

fn default_tab_idle_secs() -> u64 { 300 }
fn default_evict_check_interval_secs() -> u64 { 60 }
fn default_min_cached_tabs() -> usize { 1 }
fn default_repo_browser_ttl_secs() -> u64 { 600 }

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_tabs: 10,
            refresh_interval_secs: 60,
            scheduler_enabled: true,
            auto_evict_enabled: false,
            tab_idle_secs: default_tab_idle_secs(),
            evict_check_interval_secs: default_evict_check_interval_secs(),
            close_repo_on_evict: true,
            min_cached_tabs: default_min_cached_tabs(),
            repo_browser_ttl_secs: default_repo_browser_ttl_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: ThemeConfig { active: "dark".into() },
            diff: DiffConfig {
                algorithm: DiffAlgorithm::Myers,
                context_lines: 3,
                word_wrap: false,
                full_file: false,
                virt_threshold: 200,
                mode: DiffMode::Split,
                file_list_view: FileListView::List,
                confirm_discard: true,
                tab_width: default_tab_width(),
            },
            graph: GraphConfig {
                page_size: 500,
                show_remote_branches: true,
                show_tags: true,
                paginate: true,
                ticket_links_enabled: true,
            },
            keybindings: KeybindingsConfig::default(),
            recent_repos: Vec::new(),
            gitflow: GitFlowConfig::default(),
            cache: CacheConfig::default(),
            ticket_links: TicketLinksGlobalConfig::default(),
            issues: IssuesConfig::default(),
            mr: MrConfig::default(),
            ide: IdeConfig::default(),
            terminals: TerminalsConfig::default(),
            activity_bar: ActivityBarConfig::default(),
            status: StatusConfig::default(),
            recovery: RecoveryConfig::default(),
            missing_projects: MissingProjectsConfig::default(),
            git: GitCliConfig::default(),
            plugins_enabled: false,
            oauth: OAuthOverrides::default(),
            pipelines: PipelinesConfig::default(),
            deep_link: crate::deep_link::DeepLinkConfig::default(),
            studio: StudioSettings::default(),
            marketplace: MarketplaceConfig::default(),
            appearance: AppearanceConfig::default(),
            animations: AnimationsConfig::default(),
            commit: CommitConfig::default(),
            onboarding: OnboardingConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("arbor")
        .join("config.toml")
}

pub fn load() -> Result<AppConfig> {
    let path = config_path();
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    let content = std::fs::read_to_string(&path)?;
    let config: AppConfig = toml::from_str(&content)?;
    Ok(config)
}

pub fn save(config: &AppConfig) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
