//! `config` domain — app-level settings handlers routed through the platform
//! backend.
//!
//! The git-product config sections (diff, graph, gitflow, cache, ticket_links,
//! issues, mr, status, recovery, missing_projects, pipelines, studio, commit,
//! branches) are OWNED by `corvus-be` now (`corvus/config.toml`) — their get/set
//! handlers live in `crates/corvus/be/src/corvus_config.rs`. What stays here are
//! the **platform/global** sections that the shell still owns in `AppConfig`
//! (`profile.toml` / `oauth.toml`): OAuth overrides, the standalone graph-columns
//! layout, activity-bar, appearance, explorer (read), animations, onboarding,
//! what's-new, and the recent-repos list. Plus `evict_tab_cache`, a shell-only
//! cache op that *reads* the corvus-owned `cache` section via a thin read.
//!
//! `set_explorer_config` is **not** here: it takes an `AppHandle` and reconciles
//! the OS-global shortcut, so it stays inline in the command module as a
//! keep-shell Tauri command. No hooks fire in this domain.

use crate::config::app_config::{
    self, ActivityBarConfig, AnimationsConfig, AppearanceConfig, ExplorerConfig, OAuthOverrides,
    OnboardingConfig, WhatsNewConfig,
};
use crate::config::graph_columns::{self, GraphColumnsConfig};
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

// ── Graph columns (separate TOML, not part of AppConfig) ────────────────────
//
// Layout of the commit-graph header — column order, per-column width,
// visibility, plus the lane-track width. Persisted standalone in
// `~/.config/arbor/graph_columns.toml` so it can be reset without touching
// the rest of `config.toml`.

/// Return the persisted graph column layout, falling back to defaults when
/// the file is missing or unreadable.
#[platform::handler(program = "platform")]
fn get_graph_columns(_state: &AppState) -> Result<GraphColumnsConfig, AppError> {
    Ok(graph_columns::load())
}

/// Persist a new graph column layout.
#[platform::handler(program = "platform")]
fn set_graph_columns(_state: &AppState, config: GraphColumnsConfig) -> Result<(), AppError> {
    graph_columns::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

/// Drop the cached `git2::Repository` handle for a specific tab to free libgit2
/// internal caches.
///
/// The per-tab backend caches (stats, ticket-links) moved out with their domains
/// — they live in `corvus-be` now (stats HEAD-keyed → self-healing; ticket links
/// re-fetched on demand). What remains here is the shell-owned `RepoManager`
/// handle: if `cache.close_repo_on_evict` is enabled and the tab is not currently
/// active, drop it (transparently re-opened on next access). The `cache` section
/// is owned by corvus-be, so its `close_repo_on_evict` flag is read back with a
/// thin partial-struct read.
#[platform::handler(program = "platform")]
fn evict_tab_cache(state: &AppState, tab_id: String) -> Result<(), AppError> {
    // Read the corvus-owned `cache.close_repo_on_evict` flag (defaults true).
    let should_close = {
        #[derive(serde::Deserialize)]
        struct CacheProbe {
            #[serde(default = "default_close_on_evict")]
            close_repo_on_evict: bool,
        }
        fn default_close_on_evict() -> bool { true }
        crate::config::corvus_read::section::<CacheProbe>("cache")
            .map(|c| c.close_repo_on_evict)
            .unwrap_or(true)
    };

    if should_close {
        let active = state.active_tab_id.lock()
            .ok()
            .and_then(|g| g.clone());
        let is_active = active.as_deref() == Some(tab_id.as_str());
        if !is_active {
            if let Ok(mut mgr) = state.lock_repos() {
                mgr.evict_repo(&tab_id);
            }
        }
    }

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

/// Return the first-run onboarding tour state.
#[platform::handler(program = "platform")]
fn get_onboarding_config(state: &AppState) -> Result<OnboardingConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.onboarding.clone())
}

/// Persist the onboarding tour state (completed flag + schema version).
#[platform::handler(program = "platform")]
fn set_onboarding_config(state: &AppState, config: OnboardingConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.onboarding = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

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
