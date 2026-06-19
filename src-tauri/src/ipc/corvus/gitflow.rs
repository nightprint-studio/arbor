//! `gitflow` domain — the **config-CRUD** slice, still served in-process.
//!
//! The 9 operational Git Flow handlers (init / feature·release·hotfix
//! start·finish / status) moved to `corvus-be`. What remains here are the 6
//! config reads/writes (`get`/`set_gitflow_global_config`,
//! `set`/`clear_gitflow_repo_config`, `get_gitflow_config`,
//! `has_gitflow_repo_override`) that own the global + per-repo
//! `.arbor/config.toml` files — they stay shell-side. The pure work lives in
//! [`crate::git::gitflow`]; `effective_config` / `get_workdir` are the shared
//! helpers the survivors use.

use crate::config::{app_config, repo_config};
use crate::error::AppError;
use crate::git::gitflow::GitFlowConfig;
use crate::ipc::corvus;
use crate::AppState;

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
    {
        let mut cfg = state.lock_config()?;
        cfg.gitflow = config;
        app_config::save(&cfg)?;
    }
    // Push the new global config to corvus-be so its OOP gitflow handlers see
    // the live value (lock released first — sync_config re-reads from disk).
    crate::ipc::sync_config(state);
    Ok(())
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

#[corvus::handler]
fn has_gitflow_repo_override(state: &AppState, tab_id: String) -> Result<bool, AppError> {
    let workdir = get_workdir(state, &tab_id)?;
    let repo_cfg = crate::config::repo_config::load(&workdir).unwrap_or_default();
    Ok(repo_cfg.gitflow.is_some())
}
