//! `config` domain — app-level (`~/.config/arbor/config.toml`) and per-repo
//! (`<repo>/.arbor/config.toml`) settings handlers routed through the platform
//! backend.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline, now
//! self-registered under `program = "platform"`. Behavior (locks held, config
//! save, errors) is byte-identical; commands that took no `AppState` use
//! `_state: &AppState` to satisfy the handler macro's context arg.
//!
//! `set_explorer_config` is **not** here: it takes an `AppHandle` and reconciles
//! the OS-global shortcut, so it stays inline in the command module as a
//! keep-shell Tauri command. No hooks fire in this domain.

use crate::config::app_config::{
    self, ActivityBarConfig, AnimationsConfig, AppearanceConfig, BranchesConfig, CacheConfig,
    CommitConfig, DiffConfig, ExplorerConfig, GraphConfig, IssuesConfig, MissingProjectsConfig,
    MrConfig, OAuthOverrides, OnboardingConfig, PipelinesConfig, RecoveryConfig, StudioSettings,
    WhatsNewConfig,
};
use crate::config::graph_columns::{self, GraphColumnsConfig};
use crate::config::repo_config::{
    load as load_repo_config, save as save_repo_config, BranchGroupingConfig, RepoConfig,
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

/// Read the recovery-snapshot policy.  Used by the Settings UI and by the
/// journal module itself when computing per-file exclusions.
#[platform::handler(program = "platform")]
fn get_recovery_config(state: &AppState) -> Result<RecoveryConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.recovery.clone())
}

/// Persist a new recovery-snapshot policy to `~/.config/arbor/config.toml`.
/// Takes effect immediately for every subsequent snapshot.
#[platform::handler(program = "platform")]
fn set_recovery_config(state: &AppState, recovery: RecoveryConfig) -> Result<(), AppError> {
    {
        let mut config = state.lock_config()?;
        config.recovery = recovery;
        app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))?;
    }
    // Push the new policy to corvus-be so its OOP snapshots pick it up live
    // (best-effort; a no-op when corvus-be isn't running). The config lock is
    // released above before the round-trip.
    crate::ipc::sync_config(state);
    Ok(())
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
// `~/.config/arbor/config.toml` under `[oauth]`.  client_id is a public
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

/// Load per-repository configuration from `.arbor/config.toml` inside the repo.
#[platform::handler(program = "platform")]
fn get_repo_config(state: &AppState, tab_id: String) -> Result<RepoConfig, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    load_repo_config(&repo.path)
}

/// Persist per-repository configuration to `.arbor/config.toml`.
#[platform::handler(program = "platform")]
fn set_repo_config(state: &AppState, tab_id: String, config: RepoConfig) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    save_repo_config(&repo.path, &config)
}

// ── Local-only tag tracking ──────────────────────────────────────────────────
//
// Git has no built-in concept of "tag not yet pushed", so we persist a list
// of locally-created tag names in `.arbor/config.toml`. Removed when the tag
// is pushed (or deleted).

/// Return the list of tag names flagged as local-only for this repo.
#[platform::handler(program = "platform")]
fn list_local_only_tags(state: &AppState, tab_id: String) -> Result<Vec<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(load_repo_config(&repo.path)?.local_only_tags)
}

/// Mark a tag as locally-created and not-yet-pushed.
#[platform::handler(program = "platform")]
fn mark_tag_local(state: &AppState, tab_id: String, name: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let mut config = load_repo_config(&repo.path)?;
    if !config.local_only_tags.iter().any(|n| n == &name) {
        config.local_only_tags.push(name);
        save_repo_config(&repo.path, &config)?;
    }
    Ok(())
}

/// Mark a tag as pushed (or deleted) — removes it from the local-only list.
#[platform::handler(program = "platform")]
fn mark_tag_pushed(state: &AppState, tab_id: String, name: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let mut config = load_repo_config(&repo.path)?;
    let before = config.local_only_tags.len();
    config.local_only_tags.retain(|n| n != &name);
    if config.local_only_tags.len() != before {
        save_repo_config(&repo.path, &config)?;
    }
    Ok(())
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

/// Return the current graph configuration.
#[platform::handler(program = "platform")]
fn get_graph_config(state: &AppState) -> Result<GraphConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.graph.clone())
}

/// Persist updated graph configuration to disk.
#[platform::handler(program = "platform")]
fn set_graph_config(state: &AppState, config: GraphConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.graph = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current cache configuration.
#[platform::handler(program = "platform")]
fn get_cache_config(state: &AppState) -> Result<CacheConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.cache.clone())
}

/// Persist updated cache configuration to disk.
#[platform::handler(program = "platform")]
fn set_cache_config(state: &AppState, config: CacheConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.cache = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// ── Pipelines (global concurrency cap) ────────────────────────────────────────

/// Read the pipelines orchestrator settings (global concurrency cap, …).
#[platform::handler(program = "platform")]
fn get_pipelines_config(state: &AppState) -> Result<PipelinesConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.pipelines.clone())
}

/// Persist updated pipelines settings to disk and wake any orchestrator
/// thread parked on the concurrency condvar so a freshly-raised cap is
/// picked up by queued runs immediately (no app restart needed).
#[platform::handler(program = "platform")]
fn set_pipelines_config(state: &AppState, config: PipelinesConfig) -> Result<(), AppError> {
    let cfg_clone = {
        let mut cfg = state.lock_config()?;
        cfg.pipelines = config;
        cfg.clone()
    };
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    // Wake every queued orchestrator. Each run snapshots the cap at start, so
    // the new value applies to runs started from here on; the notify just lets
    // already-parked runs re-check their (snapshotted) cap without waiting out
    // the 250 ms poll timeout.
    state.pipeline_engine.cv.notify_all();
    Ok(())
}

// ── Studio (RON / JSON / TOML sidebar settings) ───────────────────────────────

#[platform::handler(program = "platform")]
fn get_studio_settings(state: &AppState) -> Result<StudioSettings, AppError> {
    Ok(state.lock_config()?.studio.clone())
}

#[platform::handler(program = "platform")]
fn set_studio_settings(state: &AppState, settings: StudioSettings) -> Result<(), AppError> {
    let cfg_clone = {
        let mut cfg = state.lock_config()?;
        cfg.studio = settings;
        cfg.clone()
    };
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Evict all backend cache entries for a specific tab.
///
/// Removes the tab from `ticket_caches` (the stats cache moved out with the
/// `stats` domain — it lives in `corvus-be` now, HEAD-keyed so it self-heals).
/// If `cache.close_repo_on_evict` is enabled and the tab is not currently
/// active, also drops the `git2::Repository` handle to free libgit2 internal
/// caches. The repo is transparently re-opened on next access.
#[platform::handler(program = "platform")]
fn evict_tab_cache(state: &AppState, tab_id: String) -> Result<(), AppError> {
    if let Ok(mut caches) = state.ticket_caches.lock() {
        caches.remove(&tab_id);
    }

    // Drop the git2::Repository handle if the feature flag is enabled and
    // this is not the currently active tab (evicting the active tab would
    // cause an immediate re-open on the very next command — pointless).
    let should_close = state.lock_config()
        .map(|cfg| cfg.cache.close_repo_on_evict)
        .unwrap_or(true);

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

/// Return the current missing-projects (tombstone + locate) configuration.
#[platform::handler(program = "platform")]
fn get_missing_projects_config(state: &AppState) -> Result<MissingProjectsConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.missing_projects.clone())
}

/// Persist updated missing-projects configuration to disk.
#[platform::handler(program = "platform")]
fn set_missing_projects_config(
    state: &AppState,
    config: MissingProjectsConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.missing_projects = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

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

/// Return the current issues display configuration.
#[platform::handler(program = "platform")]
fn get_issues_config(state: &AppState) -> Result<IssuesConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.issues.clone())
}

/// Persist updated issues display configuration to disk.
#[platform::handler(program = "platform")]
fn set_issues_config(state: &AppState, config: IssuesConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.issues = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current diff configuration (algorithm, context, full-file, virt threshold).
#[platform::handler(program = "platform")]
fn get_diff_config(state: &AppState) -> Result<DiffConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.diff.clone())
}

/// Persist updated diff configuration to disk.
#[platform::handler(program = "platform")]
fn set_diff_config(state: &AppState, config: DiffConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.diff = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current MR/PR Activity-timeline filter defaults.
#[platform::handler(program = "platform")]
fn get_mr_config(state: &AppState) -> Result<MrConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.mr.clone())
}

/// Persist updated MR/PR filter defaults to disk.
#[platform::handler(program = "platform")]
fn set_mr_config(state: &AppState, config: MrConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.mr = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

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

// ── Branches sidebar (global behaviour + per-repo grouping state) ───────────

/// Read the global Branches-sidebar behaviour knobs (e.g. recursive path split).
#[platform::handler(program = "platform")]
fn get_branches_config(state: &AppState) -> Result<BranchesConfig, AppError> {
    Ok(state.lock_config()?.branches.clone())
}

/// Persist updated Branches-sidebar behaviour knobs.
#[platform::handler(program = "platform")]
fn set_branches_config(state: &AppState, config: BranchesConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.branches = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Read the per-repo branch-grouping state (enabled flag + collapsed groups).
/// Convenience wrapper over `RepoConfig.branch_grouping` so the frontend
/// store doesn't have to round-trip the entire RepoConfig on every toggle.
#[platform::handler(program = "platform")]
fn get_branch_grouping(state: &AppState, tab_id: String) -> Result<BranchGroupingConfig, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(load_repo_config(&repo.path)?.branch_grouping)
}

/// Persist per-repo branch-grouping state (enabled + collapsed groups).
#[platform::handler(program = "platform")]
fn set_branch_grouping(
    state: &AppState,
    tab_id: String,
    config: BranchGroupingConfig,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let mut cfg = load_repo_config(&repo.path)?;
    cfg.branch_grouping = config;
    save_repo_config(&repo.path, &cfg)
}

/// Read host-wide commit preferences (global template fallback, …).
#[platform::handler(program = "platform")]
fn get_commit_config(state: &AppState) -> Result<CommitConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.commit.clone())
}

/// Persist updated commit preferences to disk.
#[platform::handler(program = "platform")]
fn set_commit_config(state: &AppState, config: CommitConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.commit = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}
