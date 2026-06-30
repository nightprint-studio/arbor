//! `config` domain — app-level settings handlers routed through the platform
//! backend.
//!
//! The git-product config sections (diff, graph, graph_columns, gitflow, cache,
//! ticket_links, issues, mr, status, recovery, missing_projects, pipelines, studio,
//! commit, branches, onboarding) are OWNED by `corvus-be` now (`corvus/config.toml`)
//! — their get/set handlers live in `crates/products/corvus/be/src/corvus_config.rs`.
//! Onboarding is per-product there (the git product's first-run tour); a
//! launcher-level onboarding, if ever needed, would live in the shell separately.
//! What stays here are the **platform/global** sections that the shell still owns in
//! `AppConfig` (`profile.toml` / `oauth.toml`): OAuth overrides, activity-bar,
//! appearance, explorer (read), animations, what's-new, and the recent-repos list.
//! Plus `evict_tab_cache`, a shell-only cache op that *reads* the corvus-owned
//! `cache` section via a thin read.
//!
//! `set_explorer_config` is **not** here: it takes an `AppHandle` and reconciles
//! the OS-global shortcut, so it stays inline in the command module as a
//! keep-shell Tauri command. No hooks fire in this domain.

use crate::config::app_config::{
    self, ActivityBarConfig, AnimationsConfig, AppearanceConfig, ExplorerConfig, OAuthOverrides,
    WhatsNewConfig,
};
use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;

// Cap the persisted recent-repo list. With WelcomeScreen showing 6 and the
// menubar submenu listing all of them, anything past ~10 is just clutter the
// user has to scroll past — and the persisted list grows forever otherwise.
const MAX_RECENT: usize = 10;

/// Return the list of recently opened repository paths.
#[platform::handler(program = "platform")]
fn get_recent_repos(state: &AppState) -> Result<Vec<String>, AppError> {
    let config = state.lock_config()?;
    Ok(config.recent_repos.clone())
}

/// Prepend a path to the recent repos list (normalised to forward slashes),
/// deduplicating any existing entry and capping the list at MAX_RECENT.
#[platform::handler(program = "platform")]
fn add_recent_repo(state: &AppState, path: String) -> Result<(), AppError> {
    let normalized = path.replace('\\', "/");
    let mut config = state.lock_config()?;
    config.recent_repos.retain(|p| p.replace('\\', "/") != normalized);
    config.recent_repos.insert(0, normalized);
    config.recent_repos.truncate(MAX_RECENT);
    app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

// ── OAuth overrides ───────────────────────────────────────────────────────
//
// Per-provider client_id (and host, for GitLab) overrides persisted in
// `~/.config/arbor/oauth.toml` under `[oauth]`.  client_id is a public
// OAuth identifier (RFC 6749 §2.2) and is intentionally stored in plain
// TOML — only access/refresh tokens go to the OS keychain.

/// Read the saved OAuth overrides.  Empty fields mean "use bundled defaults".
#[platform::handler(program = "platform")]
fn get_oauth_overrides(state: &AppState) -> Result<OAuthOverrides, AppError> {
    let config = state.lock_config()?;
    Ok(config.oauth.clone())
}

/// Persist OAuth client_id / host overrides.  Empty strings reset to default.
#[platform::handler(program = "platform")]
fn set_oauth_overrides(state: &AppState, overrides: OAuthOverrides) -> Result<(), AppError> {
    let mut config = state.lock_config()?;
    config.oauth = overrides;
    app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

/// Bundled OAuth defaults — exposed so the Settings UI can show them as
/// placeholder hints when an override is empty.
#[platform::handler(program = "platform")]
fn get_oauth_defaults(_state: &AppState) -> Result<OAuthDefaults, AppError> {
    Ok(OAuthDefaults {
        github_client_id: crate::git_provider::oauth::github_flow::DEFAULT_CLIENT_ID.into(),
        gitlab_client_id: crate::git_provider::oauth::gitlab_flow::DEFAULT_CLIENT_ID.into(),
        gitlab_base_host: crate::git_provider::oauth::gitlab_flow::DEFAULT_BASE_HOST.into(),
        linear_client_id: crate::auth::oauth_linear::DEFAULT_CLIENT_ID.into(),
        jira_client_id:   crate::auth::oauth_jira::DEFAULT_CLIENT_ID.into(),
    })
}

#[derive(serde::Serialize)]
pub struct OAuthDefaults {
    pub github_client_id: String,
    pub gitlab_client_id: String,
    pub gitlab_base_host: String,
    pub linear_client_id: String,
    pub jira_client_id:   String,
}

// Graph-column layout (commit-graph header: column order, width, visibility)
// moved to corvus-be — it's a git-graph concern. See `get/set_graph_columns` in
// `crates/products/corvus/be/src/corvus_config.rs`.

/// Historically dropped the shell's cached `git2::Repository` handle for an
/// inactive tab to free libgit2 memory. The launcher no longer caches git
/// handles — it keeps no `RepoManager`, and `corvus-be` (the sole git owner)
/// opens a fresh handle per operation rather than caching one — so there is
/// nothing to evict. Kept as a no-op so the frontend's tab-switch call still
/// resolves. The `cache.close_repo_on_evict` setting is now meaningless and the
/// per-tab backend caches (stats, ticket-links) live in `corvus-be`.
#[platform::handler(program = "platform")]
fn evict_tab_cache(_state: &AppState, _tab_id: String) -> Result<(), AppError> {
    Ok(())
}

// ── Activity bar (platform UI — visibility + ordering) ──────────────────────

/// Return the current activity-bar configuration.
#[platform::handler(program = "platform")]
fn get_activity_bar_config(state: &AppState) -> Result<ActivityBarConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.activity_bar.clone())
}

/// Persist updated activity-bar configuration to disk.
#[platform::handler(program = "platform")]
fn set_activity_bar_config(state: &AppState, config: ActivityBarConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.activity_bar = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// ── Appearance preferences (window control style, font scale, …) ────────────

/// Return the current appearance preferences (window control style, …).
#[platform::handler(program = "platform")]
fn get_appearance_config(state: &AppState) -> Result<AppearanceConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.appearance.clone())
}

/// Persist updated appearance preferences to disk.
#[platform::handler(program = "platform")]
fn set_appearance_config(state: &AppState, config: AppearanceConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.appearance = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the built-in file-explorer preferences (git awareness, global
/// shortcut, display defaults).
#[platform::handler(program = "platform")]
fn get_explorer_config(state: &AppState) -> Result<ExplorerConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.explorer.clone())
}

/// Read the current UI animations preferences (enabled + speed).
#[platform::handler(program = "platform")]
fn get_animations_config(state: &AppState) -> Result<AnimationsConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.animations.clone())
}

/// Persist updated animations preferences to disk.
#[platform::handler(program = "platform")]
fn set_animations_config(state: &AppState, config: AnimationsConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.animations = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// Onboarding moved to corvus-be (per-product first-run tour). See
// `get/set_onboarding_config` in `crates/products/corvus/be/src/corvus_config.rs`.

/// Return the "What's New" state (last app version the user has been
/// shown the release notes for).
#[platform::handler(program = "platform")]
fn get_whats_new_config(state: &AppState) -> Result<WhatsNewConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.whats_new.clone())
}

/// Persist the "What's New" state after the modal is dismissed.
#[platform::handler(program = "platform")]
fn set_whats_new_config(state: &AppState, config: WhatsNewConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.whats_new = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}
