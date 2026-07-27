use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

use arbor_core::prelude::{arbor_config_path, arbor_profile_path};
use crate::error::Result;

/// Top-level `AppConfig` keys the **shell** owns and persists to the per-profile
/// `profile.toml`. These are the product-agnostic UI prefs PLUS a few shell-level
/// concerns that aren't corvus's to own (`terminals`, `activity_bar`, `ide`
/// launcher, the `git` executable path — the shell shells out to git itself —
/// and the `recent_repos`/launcher recents list). The corvus git-product config
/// sections (diff, graph, gitflow, cache, …) are NOT here and NOT in AppConfig
/// anymore: corvus-be owns `corvus/config.toml`. `oauth` is the only [`GLOBAL_KEYS`]
/// entry. Single source of truth for the partition. See
/// `docs/profiles-and-product-config.md`.
const GENERIC_KEYS: &[&str] = &[
    "theme", "keybindings", "appearance", "animations",
    "whats_new", "explorer", "tyto", "plugins_enabled", "marketplace", "deep_link",
    "launcher", "terminals", "activity_bar", "ide", "git", "recent_repos", "recents",
];

/// Top-level `AppConfig` keys that are global (shared across every profile),
/// kept at the `arbor/` root rather than inside a profile. OAuth `client_id`
/// overrides are deployment identity, not a per-profile user pref.
const GLOBAL_KEYS: &[&str] = &["oauth"];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub keybindings: KeybindingsConfig,
    /// Paths of recently opened repositories (Corvus's own list, kept for its
    /// quick-switch). The cross-product history is [`AppConfig::recents`].
    #[serde(default)]
    pub recent_repos: Vec<String>,
    /// What you last worked on, across every product — the list Canopy shows.
    ///
    /// Shell-level on purpose: the per-product histories live in three different
    /// places (Corvus in `recent_repos`, Bennu's only in memory, Merula's in its
    /// own config), and two of them are unreachable unless that product's
    /// backend is running. The launcher can't start three backends to draw a
    /// list, so each product reports here as it opens something.
    #[serde(default)]
    pub recents: Vec<RecentProject>,
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
    /// `arbor://…` deep-link routing preferences.
    #[serde(default)]
    pub deep_link: crate::deep_link::DeepLinkConfig,
    /// Marketplace catalog auto-refresh policy.
    #[serde(default)]
    pub marketplace: MarketplaceConfig,
    /// Visual appearance preferences (window control style, …).
    #[serde(default)]
    pub appearance: AppearanceConfig,
    /// UI animation preferences (enable/disable, speed multiplier).
    #[serde(default)]
    pub animations: AnimationsConfig,
    // First-run onboarding tour state is **per-product** now: corvus-be owns the
    // git product's onboarding in `corvus/config.toml`. It is no longer a global
    // shell setting (see `crates/products/corvus/be/src/corvus_config.rs`). A launcher-level
    // onboarding, if ever needed, would be re-added here as its own section.
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
    /// Tyto (screen recorder) launcher-side preferences — the opt-in OS-global
    /// shortcut that opens the recorder window.
    #[serde(default)]
    pub tyto: TytoConfig,
    /// Launcher (Canopy home screen) preferences.
    #[serde(default)]
    pub launcher: LauncherConfig,
}

/// One entry of the cross-product "recently opened" history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentProject {
    /// Canopy product id that opened it (`corvus` / `bennu` / `merula`).
    pub product: String,
    /// Absolute path of the repository / project root — the identity of the entry.
    pub path: String,
    /// Display name, as the product knows it (repo name, Maven artifact, …).
    pub name: String,
    /// Unix seconds of the last open. Sorting key.
    pub opened_at: u64,
}

/// Launcher (Canopy) preferences.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LauncherConfig {
    /// Per-product launcher preferences, keyed by Canopy product id
    /// (`corvus` / `merula` / `sitta`). A product missing from the map uses the
    /// defaults (terminate on close).
    #[serde(default)]
    pub products: std::collections::HashMap<String, ProductLauncherConfig>,
    /// How the launcher opens workspace products — see [`WindowMode`].
    #[serde(default = "WindowMode::platform_default")]
    pub window_mode: WindowMode,
}

/// Where a workspace product (Corvus, Bennu, Merula) opens.
///
/// The default is per-OS rather than universal, because the platforms genuinely
/// differ: Windows and Linux give every window a taskbar button, so separate
/// windows are the better model there; macOS gives none, so a user with three
/// products open has nothing to click and lives in ⌘-Tab. Same code either way —
/// only the default differs, and the user can flip it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowMode {
    /// One window per product (the historic behaviour).
    Windows,
    /// All workspace products as tabs in a single container window.
    Tabbed,
}

impl WindowMode {
    /// Separate windows, on every platform, until the container has proven
    /// itself.
    ///
    /// It was meant to default to `Tabbed` on macOS — that is the whole point of
    /// the mode — but a default that routes EVERY product launch through one
    /// window is only safe once that window is known to open reliably: while the
    /// container was failing to build, nothing could be launched at all. Flip
    /// the macOS arm back on once the container is validated there; users who
    /// choose `tabbed` explicitly are unaffected either way.
    pub fn platform_default() -> Self {
        WindowMode::Windows
    }
}

impl Default for WindowMode {
    fn default() -> Self {
        Self::platform_default()
    }
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

/// Launcher-side File Explorer integration settings.
///
/// Only the four settings the **shell/launcher** consumes live here — the
/// OS-global shortcut (+ accelerator), whether opening always spawns a new
/// window, and whether app-wide "Reveal in File Explorer" actions route into the
/// built-in explorer. The launcher reads these even when `sitta-be` isn't running
/// (it registers the OS hotkey at boot, decides window create-vs-focus, and routes
/// reveals app-wide), so they stay here rather than in sitta's own config.
///
/// The explorer's content/UX preferences (view/sort/startup, sidebar + column
/// layout, favourites, saved searches, external-link policy, the git-awareness
/// switch) moved to `sitta-core`'s `SittaConfig`, owned out-of-process by
/// `sitta-be` (`get/set_sitta_config`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplorerConfig {
    /// Register the OS-global `Ctrl+Shift+E` shortcut that opens the dedicated
    /// explorer window. Off by default (opt-in, so Arbor doesn't claim a
    /// system-wide hotkey unprompted); toggling re-registers at runtime.
    #[serde(default)]
    pub global_shortcut: bool,
    /// Accelerator string for the global shortcut (Tauri format, e.g.
    /// `"Ctrl+Shift+E"`). Only consulted when `global_shortcut` is true.
    #[serde(default = "default_shortcut_accel")]
    pub global_shortcut_accel: String,
    /// When true, opening the explorer (shortcut / Command Palette) always
    /// spawns a NEW window; when false (default) a single window is reused and
    /// re-summoning just focuses it.
    #[serde(default)]
    pub always_new_window: bool,
    /// Route the app's "Open / Reveal in File Explorer" actions (worktree info,
    /// plugin folders, notification reveals, …) into Arbor's built-in explorer
    /// window instead of the OS file manager. Off by default — when off, those
    /// actions hand the path to the platform shell as before. The explorer's
    /// own "Reveal in File Explorer" item always uses the OS (escape hatch).
    #[serde(default)]
    pub reveal_in_builtin: bool,
}

fn default_shortcut_accel() -> String { "Ctrl+Shift+E".into() }

impl Default for ExplorerConfig {
    fn default() -> Self {
        Self {
            global_shortcut:       false,
            global_shortcut_accel: default_shortcut_accel(),
            always_new_window:     false,
            reveal_in_builtin:     false,
        }
    }
}

/// Launcher-side **Tyto** (screen recorder) settings.
///
/// Only the shell-owned bits live here — the opt-in OS-global shortcut that
/// opens the recorder window (the launcher registers it at boot and reconciles
/// it on change, like the explorer's). Tyto's capture/output preferences will be
/// owned by its own backend once it exists; they are not here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TytoConfig {
    /// Register the OS-global shortcut that opens the Tyto window. Off by default
    /// (opt-in, so Arbor doesn't claim a system-wide hotkey unprompted); toggling
    /// re-registers at runtime.
    #[serde(default)]
    pub global_shortcut: bool,
    /// Accelerator string for the global shortcut (Tauri format, e.g.
    /// `"Ctrl+Shift+R"`). Only consulted when `global_shortcut` is true.
    #[serde(default = "default_tyto_shortcut_accel")]
    pub global_shortcut_accel: String,
}

fn default_tyto_shortcut_accel() -> String { "Ctrl+Shift+R".into() }

impl Default for TytoConfig {
    fn default() -> Self {
        Self {
            global_shortcut:       false,
            global_shortcut_accel: default_tyto_shortcut_accel(),
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeybindingsConfig {
    pub bindings: std::collections::HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

// `AppConfig` stays one flat aggregate in memory — call sites read
// `cfg.<field>` unchanged — but its on-disk form is split across the
// profile × product layout (`docs/profiles-and-product-config.md`). Three
// files, partitioned by top-level key:
//   - generic (shell-owned) → arbor/profiles/<active>/profile.toml
//   - global (shared)       → arbor/oauth.toml
// The corvus git-product sections (diff, graph, gitflow, …) left AppConfig:
// corvus-be owns `arbor/profiles/<active>/corvus/config.toml` and the shell no
// longer reads or writes it here. Path resolution honours the active profile
// cell in `arbor-core`, seeded at boot.

/// Legacy monolithic config path (`arbor/config.toml`). Still *read* for the
/// one-shot migration into the split layout; never written anymore.
pub fn config_path() -> PathBuf {
    arbor_config_path("config.toml")
}

fn profile_file() -> PathBuf { arbor_profile_path("profile.toml") }
fn oauth_file() -> PathBuf { arbor_config_path("oauth.toml") }

pub fn load() -> Result<AppConfig> {
    let (profile_p, oauth_p) = (profile_file(), oauth_file());

    // Split layout present → merge the shell-owned files into one AppConfig.
    // Per-field `#[serde(default)]` lets each file carry only its own keys.
    // corvus-be's `corvus/config.toml` is NOT read here — those sections left
    // AppConfig (corvus-be owns them).
    if profile_p.exists() || oauth_p.exists() {
        let mut merged = toml::Table::new();
        for p in [&profile_p, &oauth_p] {
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
    // corvus-be owns `corvus/config.toml`; the shell writes only its own files.
    let (generic, global) = partition_table(table);
    write_toml(&profile_file(), generic)?;
    write_toml(&oauth_file(), global)?;
    Ok(())
}

/// Split the shell's config table into (generic, global) by top-level key.
/// Every `AppConfig` key is now either generic (shell-owned → profile.toml) or
/// global (→ oauth.toml); the corvus git-product sections live in corvus-be's
/// own file and never reach this function. Pure — no disk — so it's unit-testable.
fn partition_table(table: toml::Table) -> (toml::Table, toml::Table) {
    let (mut generic, mut global) = (toml::Table::new(), toml::Table::new());
    for (k, v) in table {
        if GENERIC_KEYS.contains(&k.as_str()) {
            generic.insert(k, v);
        } else if GLOBAL_KEYS.contains(&k.as_str()) {
            global.insert(k, v);
        } else {
            // No corvus bucket anymore — AppConfig holds only generic+global
            // keys. Anything unexpected falls back to generic (shell-owned).
            generic.insert(k, v);
        }
    }
    (generic, global)
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

        let (generic, global) = partition_table(table);
        // Generic UI prefs + the shell-owned concerns land in the profile bucket…
        assert!(generic.contains_key("theme"));
        assert!(generic.contains_key("appearance"));
        assert!(generic.contains_key("git"));
        assert!(generic.contains_key("recent_repos"));
        // …OAuth in the global bucket, and never crossed over.
        assert!(global.contains_key("oauth"));
        assert!(!generic.contains_key("oauth"));
        assert!(!global.contains_key("theme"));

        // Merge the two files back → deserialize → key fields survive.
        let mut merged = toml::Table::new();
        for t in [generic, global] {
            for (k, v) in t {
                merged.insert(k, v);
            }
        }
        let restored: AppConfig = toml::Value::Table(merged).try_into().unwrap();
        assert_eq!(restored.theme.active, cfg.theme.active);
        assert_eq!(restored.git.executable_path, cfg.git.executable_path);
    }

    #[test]
    fn partial_files_deserialize_via_field_defaults() {
        // A profile.toml that carries only `[theme]` must still load — the
        // missing sections fall back to their defaults.
        let only_theme: toml::Table =
            toml::from_str("[theme]\nactive = \"light\"\n").unwrap();
        let cfg: AppConfig = toml::Value::Table(only_theme).try_into().unwrap();
        assert_eq!(cfg.theme.active, "light");
        assert_eq!(cfg.appearance.font_scale, AppearanceConfig::default().font_scale);
    }
}
