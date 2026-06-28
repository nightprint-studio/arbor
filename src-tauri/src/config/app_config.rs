use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use arbor_core::prelude::{arbor_config_path, arbor_profile_path, product_path, PRODUCT_CORVUS};
use crate::error::Result;
use crate::git::gitflow::GitFlowConfig;
use corvus_git::prelude::StorageBackend;

/// Top-level `AppConfig` keys that are product-agnostic and persist to the
/// per-profile `profile.toml`. Everything not listed here (and not in
/// [`GLOBAL_KEYS`]) persists to the corvus product file — so a new corvus
/// section needs no change here, while a new *generic* section must be added.
/// Single source of truth for the partition. See
/// `docs/profiles-and-product-config.md`.
const GENERIC_KEYS: &[&str] = &[
    "theme", "keybindings", "appearance", "animations", "onboarding",
    "whats_new", "explorer", "plugins_enabled", "marketplace", "deep_link",
    "launcher",
];

/// Top-level `AppConfig` keys that are global (shared across every profile),
/// kept at the `arbor/` root rather than inside a profile. OAuth `client_id`
/// overrides are deployment identity, not a per-profile user pref.
const GLOBAL_KEYS: &[&str] = &["oauth"];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub diff: DiffConfig,
    #[serde(default)]
    pub graph: GraphConfig,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
    /// Paths of recently opened repositories.
    #[serde(default)]
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
    /// Global behaviour knobs for the Branches sidebar. The per-repo
    /// `branch_grouping.enabled` toggle decides *whether* to group; the
    /// fields here decide *how* (e.g. recursive vs single-level split).
    #[serde(default)]
    pub branches: BranchesConfig,
    /// "What's New" modal state. Tracks the last app version the user has
    /// already seen the release notes for, so the modal only auto-opens
    /// the first time after an upgrade.
    #[serde(default)]
    pub whats_new: WhatsNewConfig,
    /// Built-in File Explorer preferences (git awareness, global shortcut,
    /// display defaults). The two host-level switches are surfaced both in the
    /// SettingsPanel and in the explorer's own in-window settings page.
    #[serde(default)]
    pub explorer: ExplorerConfig,
    /// Launcher (Canopy home screen) preferences.
    #[serde(default)]
    pub launcher: LauncherConfig,
}

/// Launcher (Canopy) preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LauncherConfig {
    /// Per-product launcher preferences, keyed by Canopy product id
    /// (`corvus` / `merula` / `sitta`). A product missing from the map uses the
    /// defaults (terminate on close).
    #[serde(default)]
    pub products: std::collections::HashMap<String, ProductLauncherConfig>,
}

/// Per-product launcher preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductLauncherConfig {
    /// How closing this product's window behaves:
    /// - `false` (default) — closing the window **terminates** the product.
    /// - `true` — tray-style: closing just **hides** the window; the product
    ///   stays running (still lit in the launcher) and is terminated only via
    ///   the launcher's Stop.
    ///
    /// The launcher's Stop always force-terminates regardless of this flag, so a
    /// product can never become an un-killable background zombie.
    #[serde(default)]
    pub close_to_tray: bool,
}

/// Built-in File Explorer preferences.
///
/// `git_awareness` and `global_shortcut` are "host-level" switches — also
/// editable from the SettingsPanel. The display defaults (`default_view`,
/// `show_hidden`, `recursive_search`) used to live in `localStorage`; they are
/// persisted here so the in-explorer settings page can edit them coherently.
/// Purely ephemeral per-path state (e.g. the per-folder view-mode memory) stays
/// in `localStorage`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerConfig {
    /// Master switch for TortoiseGit-style git awareness in the file explorer
    /// (status overlays, repo-root markers, the Changes panel, branch switch).
    /// Off by default — when off the explorer issues no git IPC at all, so a
    /// plain file browse never pays a per-navigation status walk.
    #[serde(default)]
    pub git_awareness: bool,
    /// Register the OS-global `Ctrl+Shift+E` shortcut that opens the dedicated
    /// explorer window. Off by default (opt-in, so Arbor doesn't claim a
    /// system-wide hotkey unprompted); toggling re-registers at runtime.
    #[serde(default)]
    pub global_shortcut: bool,
    /// Default view mode applied to not-yet-visited folders:
    /// `details` | `medium` | `large` | `xlarge`.
    #[serde(default = "default_explorer_view")]
    pub default_view: String,
    /// Show dot-prefixed (hidden) entries by default.
    #[serde(default)]
    pub show_hidden: bool,
    /// Default state of recursive (subfolder) search.
    #[serde(default)]
    pub recursive_search: bool,
    /// Accelerator string for the global shortcut (Tauri format, e.g.
    /// `"Ctrl+Shift+E"`). Only consulted when `global_shortcut` is true.
    #[serde(default = "default_shortcut_accel")]
    pub global_shortcut_accel: String,
    /// Default column the listing sorts by: `name` | `modified` | `size`.
    #[serde(default = "default_explorer_sort")]
    pub default_sort: String,
    /// Default sort direction (ascending when true).
    #[serde(default = "default_true_sort")]
    pub sort_ascending: bool,
    /// What a freshly-opened explorer tab shows: `overview` (the dashboard) or
    /// `last` (re-open the most recent folder, if any).
    #[serde(default = "default_explorer_startup")]
    pub startup: String,
    /// When true, opening the explorer (shortcut / Command Palette) always
    /// spawns a NEW window; when false (default) a single window is reused and
    /// re-summoning just focuses it.
    #[serde(default)]
    pub always_new_window: bool,
    /// Maximum number of recent folders kept in the sidebar (clamped 1–50).
    #[serde(default = "default_max_recents")]
    pub max_recents: u32,
    /// Sidebar section order + visibility. Empty → built-in order, all shown.
    /// Unknown ids are ignored; sections missing from the list are appended in
    /// their built-in position and shown.
    #[serde(default)]
    pub sidebar_sections: Vec<ExplorerSectionConfig>,
    /// Allow opening generic external links typed in the explorer address bar
    /// (custom schemes like `vscode://`, `mailto:`, `slack://`) via the OS
    /// default handler. Off by default — each open still prompts unless the
    /// scheme was remembered. `arbor://` deep links are handled separately.
    #[serde(default)]
    pub open_external_links: bool,
    /// Additionally allow plain web links (`http://`, `https://`) from the
    /// address bar to open in the default browser. Gated behind
    /// `open_external_links` AND off by default (web links are the broadest
    /// surface, so they're opt-in on top of the master switch).
    #[serde(default)]
    pub open_web_links: bool,
    /// Schemes the user chose "remember" for in the external-link confirm
    /// prompt (lower-cased, e.g. `["vscode", "https"]`). Future links of a
    /// remembered scheme open without prompting.
    #[serde(default)]
    pub remembered_external_schemes: Vec<String>,
    /// Route the app's "Open / Reveal in File Explorer" actions (worktree info,
    /// plugin folders, notification reveals, …) into Arbor's built-in explorer
    /// window instead of the OS file manager. Off by default — when off, those
    /// actions hand the path to the platform shell as before. The explorer's
    /// own "Reveal in File Explorer" item always uses the OS (escape hatch).
    #[serde(default)]
    pub reveal_in_builtin: bool,
    /// Details-view column order + visibility. Empty → built-in order with the
    /// default-on set shown. Unknown ids are ignored; columns missing from the
    /// list are appended in their built-in position with their default state.
    /// `name` is always shown first regardless of what's stored.
    #[serde(default)]
    pub columns: Vec<ExplorerColumnConfig>,
    /// User-pinned favourite folders shown in the sidebar's Favourites section,
    /// in addition to the OS standard locations. Absolute paths.
    #[serde(default)]
    pub pinned_favourites: Vec<String>,
    /// Saved searches surfaced as their own sidebar section. Each captures a
    /// query + filters + (optional) root folder and re-runs on click.
    #[serde(default)]
    pub saved_searches: Vec<ExplorerSavedSearch>,
}

/// One sidebar section's persisted order + visibility. Mirrors
/// [`ActivityBarItemConfig`] for the explorer's own sidebar.
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
    pub id:    String,
    pub name:  String,
    #[serde(default)]
    pub query: String,
    /// Folder the search runs in. Empty → the current folder at run time.
    #[serde(default)]
    pub root:  String,
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
    pub modified_after:  Option<i64>,
    #[serde(default)]
    pub modified_before: Option<i64>,
}

fn default_explorer_view() -> String { "details".into() }
fn default_shortcut_accel() -> String { "Ctrl+Shift+E".into() }
fn default_explorer_sort() -> String { "name".into() }
fn default_true_sort() -> bool { true }
fn default_explorer_startup() -> String { "overview".into() }
fn default_max_recents() -> u32 { 10 }

impl Default for ExplorerConfig {
    fn default() -> Self {
        Self {
            git_awareness:        false,
            global_shortcut:      false,
            default_view:         default_explorer_view(),
            show_hidden:          false,
            recursive_search:     false,
            global_shortcut_accel: default_shortcut_accel(),
            default_sort:         default_explorer_sort(),
            sort_ascending:       true,
            startup:              default_explorer_startup(),
            always_new_window:    false,
            max_recents:          default_max_recents(),
            sidebar_sections:     Vec::new(),
            open_external_links:  false,
            open_web_links:       false,
            remembered_external_schemes: Vec::new(),
            reveal_in_builtin:    false,
            columns:              Vec::new(),
            pinned_favourites:    Vec::new(),
            saved_searches:       Vec::new(),
        }
    }
}

/// "What's New" modal state. The frontend compares the current app version
/// (from `tauri.conf.json` via `get_app_info`) against `last_seen_version`
/// on boot and auto-opens the modal when they differ. A fresh install
/// (no stored value) saves the current version silently — first-time users
/// don't get a popup on initial launch.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WhatsNewConfig {
    /// Last app version the user has been shown the What's New modal for.
    /// `None` means the user has never seen it (fresh install).
    #[serde(default)]
    pub last_seen_version: Option<String>,
}

/// Global Branches-sidebar behaviour. Per-repo on/off lives in
/// `RepoConfig.branch_grouping.enabled` — these knobs apply to every
/// repo that has grouping turned on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchesConfig {
    /// When `true`, group splits on every `/` so `feature/auth/login`
    /// renders as `feature → auth → login`. When `false`, only the first
    /// `/` splits so the same branch renders as `feature → auth/login`.
    /// Recursive matches GitKraken / Fork; single-level matches JetBrains.
    /// Default: true.
    #[serde(default = "default_true_branches")]
    pub grouping_recursive: bool,
}

fn default_true_branches() -> bool { true }

impl Default for BranchesConfig {
    fn default() -> Self { Self { grouping_recursive: true } }
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
    /// Maximum number of dialogs that can sit minimized in the status-bar
    /// parked-dialogs panel at the same time. New minimize attempts past
    /// this cap are refused with a toast (non-destructive — no parked
    /// dialog is auto-closed). Clamped to `[1, 20]` on read.
    #[serde(default = "default_parked_modals_max")]
    pub parked_modals_max: u32,
    /// IntelliJ-style "compact middle packages" for file trees. When `true`,
    /// chains of single-child directories collapse into one row (e.g.
    /// `a/b/c/foo.md` shows as `a/b/c` if `a` and `b` only contain the next
    /// segment). Applies to the file panel, stage area, commit detail diff
    /// list, and conflict file sidebar. On by default.
    #[serde(default = "default_true_compact_dirs")]
    pub compact_file_tree_dirs: bool,
}

fn default_true_compact_dirs() -> bool { true }

fn default_window_controls_style() -> String { "mac".into() }
fn default_font_scale() -> f32 { 1.0 }
fn default_parked_modals_max() -> u32 { 5 }

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
            parked_modals_max:     default_parked_modals_max(),
            compact_file_tree_dirs: true,
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

impl Default for ThemeConfig {
    fn default() -> Self {
        Self { active: "dark".into() }
    }
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

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            algorithm:      DiffAlgorithm::Myers,
            context_lines:  3,
            word_wrap:      false,
            full_file:      false,
            virt_threshold: default_virt_threshold(),
            mode:           default_diff_mode_split(),
            file_list_view: FileListView::default(),
            confirm_discard: default_true_diff(),
            tab_width:      default_tab_width(),
        }
    }
}

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

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            page_size:            500,
            show_remote_branches: true,
            show_tags:            true,
            paginate:             true,
            ticket_links_enabled: true,
        }
    }
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
            theme: ThemeConfig::default(),
            diff: DiffConfig::default(),
            graph: GraphConfig::default(),
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
            branches: BranchesConfig::default(),
            whats_new: WhatsNewConfig::default(),
            explorer: ExplorerConfig::default(),
            launcher: LauncherConfig::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

// `AppConfig` stays one flat aggregate in memory — call sites read
// `cfg.<field>` unchanged — but its on-disk form is split across the
// profile × product layout (`docs/profiles-and-product-config.md`). Three
// files, partitioned by top-level key:
//   - generic (product-agnostic) → arbor/profiles/<active>/profile.toml
//   - global (shared)            → arbor/oauth.toml
//   - corvus (git product)       → arbor/profiles/<active>/corvus/config.toml
// The partition has ONE source of truth: GENERIC_KEYS + GLOBAL_KEYS (everything
// else is corvus). Path resolution honours the active profile cell in
// `arbor-core`, seeded at boot.

/// Legacy monolithic config path (`arbor/config.toml`). Still *read* for the
/// one-shot migration into the split layout; never written anymore.
pub fn config_path() -> PathBuf {
    arbor_config_path("config.toml")
}

fn profile_file() -> PathBuf { arbor_profile_path("profile.toml") }
fn corvus_file() -> PathBuf { product_path(PRODUCT_CORVUS, "config.toml") }
fn oauth_file() -> PathBuf { arbor_config_path("oauth.toml") }

pub fn load() -> Result<AppConfig> {
    let (profile_p, corvus_p, oauth_p) = (profile_file(), corvus_file(), oauth_file());

    // Split layout present → merge the per-file tables back into one AppConfig.
    // Per-field `#[serde(default)]` lets each file carry only its own keys.
    if profile_p.exists() || corvus_p.exists() {
        let mut merged = toml::Table::new();
        for p in [&profile_p, &corvus_p, &oauth_p] {
            if p.exists() {
                let tbl: toml::Table = toml::from_str(&std::fs::read_to_string(p)?)?;
                for (k, v) in tbl {
                    merged.insert(k, v);
                }
            }
        }
        return Ok(toml::Value::Table(merged).try_into()?);
    }

    // One-shot migration: a pre-profiles install has a flat config.toml. Load it
    // and persist into the split layout so later boots take the path above. The
    // legacy file is left in place as a backup.
    // Gated to the DEFAULT profile: the flat config is conceptually the default
    // profile's, so a freshly-created non-default profile must start from
    // built-in defaults, not inherit the legacy monolith.
    let legacy = config_path();
    if arbor_core::prelude::active_profile() == arbor_core::prelude::DEFAULT_PROFILE
        && legacy.exists()
    {
        let config: AppConfig = toml::from_str(&std::fs::read_to_string(&legacy)?)?;
        let _ = save(&config);
        return Ok(config);
    }

    Ok(AppConfig::default())
}

pub fn save(config: &AppConfig) -> Result<()> {
    let table = match toml::Value::try_from(config)? {
        toml::Value::Table(t) => t,
        other => {
            return Err(crate::error::AppError::Other(format!(
                "config did not serialize to a TOML table: {other:?}"
            )))
        }
    };
    let (generic, global, corvus) = partition_table(table);
    write_toml(&profile_file(), generic)?;
    write_toml(&corvus_file(), corvus)?;
    write_toml(&oauth_file(), global)?;
    Ok(())
}

/// Split a flat config table into (generic, global, corvus) by top-level key.
/// Pure — no disk — so the partition is unit-testable.
fn partition_table(table: toml::Table) -> (toml::Table, toml::Table, toml::Table) {
    let (mut generic, mut global, mut corvus) =
        (toml::Table::new(), toml::Table::new(), toml::Table::new());
    for (k, v) in table {
        let bucket = if GENERIC_KEYS.contains(&k.as_str()) {
            &mut generic
        } else if GLOBAL_KEYS.contains(&k.as_str()) {
            &mut global
        } else {
            &mut corvus
        };
        bucket.insert(k, v);
    }
    (generic, global, corvus)
}

/// Serialize one partitioned table to `path`, creating parent dirs. An empty
/// table still writes (an empty file) so the layout materializes predictably.
fn write_toml(path: &Path, table: toml::Table) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(&toml::Value::Table(table))?;
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_partitions_keys_then_merge_round_trips() {
        let cfg = AppConfig::default();
        let table = match toml::Value::try_from(&cfg).unwrap() {
            toml::Value::Table(t) => t,
            _ => panic!("AppConfig must serialize to a table"),
        };

        let (generic, global, corvus) = partition_table(table);
        // Generic UI prefs land in the profile bucket…
        assert!(generic.contains_key("theme"));
        assert!(generic.contains_key("appearance"));
        // …OAuth in the global bucket…
        assert!(global.contains_key("oauth"));
        // …and git-domain sections in the corvus bucket (never in generic).
        assert!(corvus.contains_key("diff"));
        assert!(corvus.contains_key("gitflow"));
        assert!(!corvus.contains_key("theme"));
        assert!(!generic.contains_key("diff"));

        // Merge the three files back → deserialize → key fields survive.
        let mut merged = toml::Table::new();
        for t in [generic, global, corvus] {
            for (k, v) in t {
                merged.insert(k, v);
            }
        }
        let restored: AppConfig = toml::Value::Table(merged).try_into().unwrap();
        assert_eq!(restored.theme.active, cfg.theme.active);
        assert_eq!(restored.diff.context_lines, cfg.diff.context_lines);
        assert_eq!(restored.graph.page_size, cfg.graph.page_size);
    }

    #[test]
    fn partial_files_deserialize_via_field_defaults() {
        // A profile.toml that carries only `[theme]` must still load — the
        // missing corvus/global sections fall back to their defaults.
        let only_theme: toml::Table =
            toml::from_str("[theme]\nactive = \"light\"\n").unwrap();
        let cfg: AppConfig = toml::Value::Table(only_theme).try_into().unwrap();
        assert_eq!(cfg.theme.active, "light");
        assert_eq!(cfg.diff.context_lines, DiffConfig::default().context_lines);
    }
}
