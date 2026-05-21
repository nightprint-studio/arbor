use tauri::State;
use crate::error::AppError;
use crate::config::repo_config::{RepoConfig, load as load_repo_config, save as save_repo_config};
use crate::config::app_config::{self, ActivityBarConfig, CacheConfig, DiffConfig, GraphConfig, IssuesConfig, MissingProjectsConfig, MrConfig, OAuthOverrides, PipelinesConfig, RecoveryConfig, StudioSettings};
use crate::AppState;

// Cap the persisted recent-repo list. With WelcomeScreen showing 6 and the
// menubar submenu listing all of them, anything past ~10 is just clutter the
// user has to scroll past — and the persisted list grows forever otherwise.
const MAX_RECENT: usize = 10;

/// Return the list of recently opened repository paths.
#[tauri::command]
pub fn get_recent_repos(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    let config = state.lock_config()?;
    Ok(config.recent_repos.clone())
}

/// Read the recovery-snapshot policy.  Used by the Settings UI and by the
/// journal module itself when computing per-file exclusions.
#[tauri::command]
pub fn get_recovery_config(state: State<'_, AppState>) -> Result<RecoveryConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.recovery.clone())
}

/// Persist a new recovery-snapshot policy to `~/.config/arbor/config.toml`.
/// Takes effect immediately for every subsequent snapshot.
#[tauri::command]
pub fn set_recovery_config(
    state: State<'_, AppState>,
    recovery: RecoveryConfig,
) -> Result<(), AppError> {
    let mut config = state.lock_config()?;
    config.recovery = recovery;
    app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

/// Prepend a path to the recent repos list (normalised to forward slashes),
/// deduplicating any existing entry and capping the list at MAX_RECENT.
#[tauri::command]
pub fn add_recent_repo(state: State<'_, AppState>, path: String) -> Result<(), AppError> {
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
#[tauri::command]
pub fn get_oauth_overrides(state: State<'_, AppState>) -> Result<OAuthOverrides, AppError> {
    let config = state.lock_config()?;
    Ok(config.oauth.clone())
}

/// Persist OAuth client_id / host overrides.  Empty strings reset to default.
#[tauri::command]
pub fn set_oauth_overrides(
    state: State<'_, AppState>,
    overrides: OAuthOverrides,
) -> Result<(), AppError> {
    let mut config = state.lock_config()?;
    config.oauth = overrides;
    app_config::save(&config).map_err(|e| AppError::Other(e.to_string()))
}

/// Bundled OAuth defaults — exposed so the Settings UI can show them as
/// placeholder hints when an override is empty.
#[tauri::command]
pub fn get_oauth_defaults() -> Result<OAuthDefaults, AppError> {
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
#[tauri::command]
pub fn get_repo_config(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<RepoConfig, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    load_repo_config(&repo.path)
}

/// Persist per-repository configuration to `.arbor/config.toml`.
#[tauri::command]
pub fn set_repo_config(
    state: State<'_, AppState>,
    tab_id: String,
    config: RepoConfig,
) -> Result<(), AppError> {
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
#[tauri::command]
pub fn list_local_only_tags(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(load_repo_config(&repo.path)?.local_only_tags)
}

/// Mark a tag as locally-created and not-yet-pushed.
#[tauri::command]
pub fn mark_tag_local(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<(), AppError> {
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
#[tauri::command]
pub fn mark_tag_pushed(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<(), AppError> {
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

/// Return the current graph configuration.
#[tauri::command]
pub fn get_graph_config(state: State<'_, AppState>) -> Result<GraphConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.graph.clone())
}

/// Persist updated graph configuration to disk.
#[tauri::command]
pub fn set_graph_config(
    state: State<'_, AppState>,
    config: GraphConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.graph = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current cache configuration.
#[tauri::command]
pub fn get_cache_config(state: State<'_, AppState>) -> Result<CacheConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.cache.clone())
}

/// Persist updated cache configuration to disk.
#[tauri::command]
pub fn set_cache_config(
    state: State<'_, AppState>,
    config: CacheConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.cache = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// ── Pipelines (global concurrency cap) ────────────────────────────────────────

/// Read the pipelines orchestrator settings (global concurrency cap, …).
#[tauri::command]
pub fn get_pipelines_config(state: State<'_, AppState>) -> Result<PipelinesConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.pipelines.clone())
}

/// Persist updated pipelines settings to disk and wake any orchestrator
/// thread parked on the concurrency condvar so a freshly-raised cap is
/// picked up by queued runs immediately (no app restart needed).
#[tauri::command]
pub fn set_pipelines_config(
    state: State<'_, AppState>,
    config: PipelinesConfig,
) -> Result<(), AppError> {
    let cfg_clone = {
        let mut cfg = state.lock_config()?;
        cfg.pipelines = config;
        cfg.clone()
    };
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))?;
    // Wake every queued orchestrator. The acquire loop re-reads the cap on
    // each iteration so a higher value lets parked runs progress in the
    // next tick rather than after the 250 ms poll timeout.
    state.pipeline_cv.notify_all();
    Ok(())
}

// ── Studio (RON / JSON / TOML sidebar settings) ───────────────────────────────

#[tauri::command]
pub fn get_studio_settings(state: State<'_, AppState>) -> Result<StudioSettings, AppError> {
    Ok(state.lock_config()?.studio.clone())
}

#[tauri::command]
pub fn set_studio_settings(
    state:    State<'_, AppState>,
    settings: StudioSettings,
) -> Result<(), AppError> {
    let cfg_clone = {
        let mut cfg = state.lock_config()?;
        cfg.studio = settings;
        cfg.clone()
    };
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Evict all backend cache entries for a specific tab.
///
/// Removes the tab from `stats_cache` and `ticket_caches`. If
/// `cache.close_repo_on_evict` is enabled and the tab is not currently active,
/// also drops the `git2::Repository` handle to free libgit2 internal caches.
/// The repo is transparently re-opened on next access.
#[tauri::command]
pub fn evict_tab_cache(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    if let Ok(mut cache) = state.stats_cache.lock() {
        cache.remove(&tab_id);
    }
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
#[tauri::command]
pub fn get_missing_projects_config(state: State<'_, AppState>) -> Result<MissingProjectsConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.missing_projects.clone())
}

/// Persist updated missing-projects configuration to disk.
#[tauri::command]
pub fn set_missing_projects_config(
    state: State<'_, AppState>,
    config: MissingProjectsConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.missing_projects = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current activity-bar configuration.
#[tauri::command]
pub fn get_activity_bar_config(state: State<'_, AppState>) -> Result<ActivityBarConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.activity_bar.clone())
}

/// Persist updated activity-bar configuration to disk.
#[tauri::command]
pub fn set_activity_bar_config(
    state: State<'_, AppState>,
    config: ActivityBarConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.activity_bar = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current issues display configuration.
#[tauri::command]
pub fn get_issues_config(state: State<'_, AppState>) -> Result<IssuesConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.issues.clone())
}

/// Persist updated issues display configuration to disk.
#[tauri::command]
pub fn set_issues_config(
    state: State<'_, AppState>,
    config: IssuesConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.issues = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current diff configuration (algorithm, context, full-file, virt threshold).
#[tauri::command]
pub fn get_diff_config(state: State<'_, AppState>) -> Result<DiffConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.diff.clone())
}

/// Persist updated diff configuration to disk.
#[tauri::command]
pub fn set_diff_config(
    state: State<'_, AppState>,
    config: DiffConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.diff = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

/// Return the current MR/PR Activity-timeline filter defaults.
#[tauri::command]
pub fn get_mr_config(state: State<'_, AppState>) -> Result<MrConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.mr.clone())
}

/// Persist updated MR/PR filter defaults to disk.
#[tauri::command]
pub fn set_mr_config(
    state: State<'_, AppState>,
    config: MrConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.mr = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}
