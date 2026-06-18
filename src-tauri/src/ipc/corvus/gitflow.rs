//! `gitflow` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. Behavior
//! (locks held, errors, the effective-config resolution) is byte-identical —
//! only the call path changed.
//!
//! The pure git work already lives in the reusable shell module
//! [`crate::git::gitflow`], so handlers delegate to it directly (no crate
//! extraction). The `on_flow_*` lifecycle hooks are fire-and-forget and are
//! fired inline by each handler with first-hand typed data, after the repo
//! lock scope has been dropped (firing while `lock_repos()` is held would
//! deadlock, since Lua hooks may call back into git ops).

use crate::config::{app_config, repo_config};
use crate::error::AppError;
use crate::git::gitflow::{FlowFinishResult, FlowStartResult, GitFlowConfig, GitFlowStatus};
use crate::ipc::corvus;
use crate::AppState;
use serde_json::json;

// ---------------------------------------------------------------------------
// Helpers — resolve the effective Git Flow config / workdir for a tab
// ---------------------------------------------------------------------------

/// Returns the per-repo config if one exists, otherwise falls back to the
/// global AppConfig default.
fn effective_config(state: &AppState, tab_id: &str) -> Result<GitFlowConfig, AppError> {
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
fn get_workdir(state: &AppState, tab_id: &str) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(tab_id)?;
    Ok(repo
        .inner()
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Config commands
// ---------------------------------------------------------------------------

#[corvus::handler]
fn get_gitflow_config(state: &AppState, tab_id: String) -> Result<GitFlowConfig, AppError> {
    effective_config(state, &tab_id)
}

#[corvus::handler]
fn get_gitflow_global_config(state: &AppState) -> Result<GitFlowConfig, AppError> {
    let cfg = state.lock_config()?;
    Ok(cfg.gitflow.clone())
}

#[corvus::handler]
fn set_gitflow_global_config(state: &AppState, config: GitFlowConfig) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.gitflow = config;
    app_config::save(&cfg)
}

#[corvus::handler]
fn set_gitflow_repo_config(
    state: &AppState,
    tab_id: String,
    config: GitFlowConfig,
) -> Result<(), AppError> {
    let workdir = get_workdir(state, &tab_id)?;
    let mut repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = Some(config);
    repo_config::save(&workdir, &repo_cfg)
}

#[corvus::handler]
fn clear_gitflow_repo_config(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let workdir = get_workdir(state, &tab_id)?;
    let mut repo_cfg = repo_config::load(&workdir).unwrap_or_default();
    repo_cfg.gitflow = None;
    repo_config::save(&workdir, &repo_cfg)
}

// ---------------------------------------------------------------------------
// Status
// ---------------------------------------------------------------------------

#[corvus::handler]
fn gitflow_get_status(state: &AppState, tab_id: String) -> Result<GitFlowStatus, AppError> {
    let config = effective_config(state, &tab_id)?;
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::gitflow::get_gitflow_status(repo.inner(), &config)
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

#[corvus::handler]
fn gitflow_init(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let config = effective_config(state, &tab_id)?;
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::gitflow_init(repo.inner(), &config)?;
    }
    // Fire inline now that the repo lock scope is dropped.
    state.fire_hook("on_flow_init", json!({ "tab_id": tab_id }));
    Ok(())
}

#[corvus::handler]
fn gitflow_init_create_main(
    state: &AppState,
    tab_id: String,
    from_initial: bool,
) -> Result<(), AppError> {
    let config = effective_config(state, &tab_id)?;
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::gitflow_init_create_main(repo.inner(), &config, from_initial)?;
    }
    // Fire inline now that the repo lock scope is dropped.
    state.fire_hook("on_flow_init", json!({ "tab_id": tab_id }));
    Ok(())
}

// ---------------------------------------------------------------------------
// Feature
// ---------------------------------------------------------------------------

#[corvus::handler]
fn gitflow_feature_start(
    state: &AppState,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::feature_start(repo.inner(), &config, &name)?
    };
    // Fire inline now that the repo lock scope is dropped (base_branch from result).
    state.fire_hook(
        "on_flow_feature_start",
        json!({ "tab_id": tab_id, "name": name, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[corvus::handler]
fn gitflow_feature_finish(
    state: &AppState,
    tab_id: String,
    name: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::feature_finish_or_pr(repo.inner(), &config, &name, force_pr)?
    };
    // Fire inline now that the repo lock scope is dropped.
    state.fire_hook(
        "on_flow_feature_finish",
        json!({ "tab_id": tab_id, "name": name }),
    );
    Ok(result)
}

// ---------------------------------------------------------------------------
// Release
// ---------------------------------------------------------------------------

#[corvus::handler]
fn gitflow_release_start(
    state: &AppState,
    tab_id: String,
    version: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::release_start(repo.inner(), &config, &version)?
    };
    // Fire inline now that the repo lock scope is dropped (base_branch from result).
    state.fire_hook(
        "on_flow_release_start",
        json!({ "tab_id": tab_id, "version": version, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[corvus::handler]
fn gitflow_release_finish(
    state: &AppState,
    tab_id: String,
    version: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::release_finish_or_pr(repo.inner(), &config, &version, &tag_message, force_pr)?
    };
    // Fire inline now that the repo lock scope is dropped.
    state.fire_hook(
        "on_flow_release_finish",
        json!({ "tab_id": tab_id, "version": version }),
    );
    Ok(result)
}

// ---------------------------------------------------------------------------
// Hotfix
// ---------------------------------------------------------------------------

#[corvus::handler]
fn gitflow_hotfix_start(
    state: &AppState,
    tab_id: String,
    name: String,
) -> Result<FlowStartResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::hotfix_start(repo.inner(), &config, &name)?
    };
    // Fire inline now that the repo lock scope is dropped (base_branch from result).
    state.fire_hook(
        "on_flow_hotfix_start",
        json!({ "tab_id": tab_id, "name": name, "base_branch": result.base_branch }),
    );
    Ok(result)
}

#[corvus::handler]
fn gitflow_hotfix_finish(
    state: &AppState,
    tab_id: String,
    name: String,
    tag_message: String,
    force_pr: bool,
) -> Result<FlowFinishResult, AppError> {
    let config = effective_config(state, &tab_id)?;
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::gitflow::hotfix_finish_or_pr(repo.inner(), &config, &name, &tag_message, force_pr)?
    };
    // Fire inline now that the repo lock scope is dropped.
    state.fire_hook(
        "on_flow_hotfix_finish",
        json!({ "tab_id": tab_id, "name": name }),
    );
    Ok(result)
}

#[corvus::handler]
fn has_gitflow_repo_override(state: &AppState, tab_id: String) -> Result<bool, AppError> {
    let workdir = get_workdir(state, &tab_id)?;
    let repo_cfg = crate::config::repo_config::load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.is_some())
}
