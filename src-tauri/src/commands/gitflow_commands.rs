use tauri::State;

use crate::error::AppError;
use crate::git::gitflow::{GitFlowConfig, GitFlowStatus, FlowFinishResult, FlowStartResult};
use crate::config::{app_config, repo_config};
use crate::AppState;

// ---------------------------------------------------------------------------
// Helper — resolve the effective Git Flow config for a tab
// ---------------------------------------------------------------------------

/// Returns the per-repo config if one exists, otherwise falls back to the
/// global AppConfig default.
fn effective_config(state: &State<'_, AppState>, tab_id: &str) -> Result<GitFlowConfig, AppError> {
    // 1. Get global config (hold lock briefly).
    let global_cfg = {
        let cfg = state.lock_config()?;
        cfg.gitflow.clone()
    };

    // 2. Get the repo workdir to load the per-repo config.
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(tab_id)?;
        repo.inner()
            .workdir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    };

    // 3. Load per-repo config; use its gitflow override if present.
    let repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.unwrap_or(global_cfg))
}

/// Get the repo workdir string (releases repos mutex immediately).
fn get_workdir(state: &State<'_, AppState>, tab_id: &str) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(tab_id)?;
    Ok(repo.inner()
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default())
}

/// Fire a Git Flow lifecycle hook on all enabled plugins.
fn fire_hook(state: &State<'_, AppState>, hook: &str, ctx: serde_json::Value) {
    if let Ok(host) = state.plugin_host.lock() {
        let _ = host.fire_hook(hook, &ctx.to_string());
    }
}

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_gitflow_config(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<GitFlowConfig, AppError> {
    effective_config(&state, &tab_id)
}

#[tauri::command]
pub fn get_gitflow_global_config(
    state: State<'_, AppState>,
) -> Result<GitFlowConfig, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.gitflow.clone())
}

#[tauri::command]
pub fn set_gitflow_global_config(
    state: State<'_, AppState>,
    config: GitFlowConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.gitflow = config;
    app_config::save(&cfg)
}

#[tauri::command]
pub fn set_gitflow_repo_config(
    state: State<'_, AppState>,
    tab_id: String,
    config: GitFlowConfig,
) -> Result<(), AppError> {
    let workdir = get_workdir(&state, &tab_id)?;
    let mut repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = Some(config);
    repo_config::save(&workdir, &repo_cfg)
}

#[tauri::command]
pub fn clear_gitflow_repo_config(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let workdir = get_workdir(&state, &tab_id)?;
    let mut repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = None;
    repo_config::save(&workdir, &repo_cfg)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gitflow_get_status(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<GitFlowStatus, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::gitflow::get_gitflow_status(repo.inner(), &config)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gitflow_init(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let config = effective_config(&state, &tab_id)?;
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::gitflow_init(repo.inner(), &config)?;
    }
    fire_hook(&state, "on_flow_init", serde_json::json!({ "tab_id": tab_id }));
    Ok(())
}

#[tauri::command]
pub fn gitflow_init_create_main(
    state: State<'_, AppState>,
    tab_id: String,
    from_initial: bool,
) -> Result<(), AppError> {
    let config = effective_config(&state, &tab_id)?;
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::gitflow_init_create_main(repo.inner(), &config, from_initial)?;
    }
    fire_hook(&state, "on_flow_init", serde_json::json!({ "tab_id": tab_id }));
    Ok(())
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gitflow_feature_start(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::feature_start(repo.inner(), &config, &name)?
    };
    fire_hook(&state, "on_flow_feature_start", serde_json::json!({
        "tab_id": tab_id, "name": name, "base_branch": result.base_branch,
    }));
    Ok(result)
}

#[tauri::command]
pub fn gitflow_feature_finish(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::feature_finish_or_pr(repo.inner(), &config, &name, force_pr)?
    };
    fire_hook(&state, "on_flow_feature_finish", serde_json::json!({ "tab_id": tab_id, "name": name }));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gitflow_release_start(
    state: State<'_, AppState>,
    tab_id: String,
    version: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::release_start(repo.inner(), &config, &version)?
    };
    fire_hook(&state, "on_flow_release_start", serde_json::json!({
        "tab_id": tab_id, "version": version, "base_branch": result.base_branch,
    }));
    Ok(result)
}

#[tauri::command]
pub fn gitflow_release_finish(
    state: State<'_, AppState>,
    tab_id: String,
    version: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::release_finish_or_pr(repo.inner(), &config, &version, &tag_message, force_pr)?
    };
    fire_hook(&state, "on_flow_release_finish", serde_json::json!({ "tab_id": tab_id, "version": version }));
    Ok(result)
}

// ---------------------------------------------------------------------------
// Hotfix
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn gitflow_hotfix_start(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::hotfix_start(repo.inner(), &config, &name)?
    };
    fire_hook(&state, "on_flow_hotfix_start", serde_json::json!({
        "tab_id": tab_id, "name": name, "base_branch": result.base_branch,
    }));
    Ok(result)
}

#[tauri::command]
pub fn gitflow_hotfix_finish(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(&state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::hotfix_finish_or_pr(repo.inner(), &config, &name, &tag_message, force_pr)?
    };
    fire_hook(&state, "on_flow_hotfix_finish", serde_json::json!({ "tab_id": tab_id, "name": name }));
    Ok(result)
}

#[tauri::command]
pub fn has_gitflow_repo_override(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<bool, AppError> {
    let workdir = get_workdir(&state, &tab_id)?;
    let repo_cfg = crate::config::repo_config::load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.is_some())
}
