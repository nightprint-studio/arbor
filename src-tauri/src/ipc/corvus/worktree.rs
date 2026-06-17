//! `worktree` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! worktree git work (enumeration, add/remove, project-type detection) now
//! lives in [`corvus_git::worktree`], reached through the shell wrapper
//! `crate::git::worktree`; the IDE launch / per-repo + global IDE config logic
//! stays shell-side (process-spawn / config concerns, not git). Behavior (locks
//! held, subprocess shelling, config round-trips, errors) is byte-identical.
//!
//! No hooks fire in this domain.
//!
//! NOT migrated (stays inline in `worktree_commands`, handled by a later
//! emit/seam pass): `start_ide_detection` — it takes an `AppHandle` and emits
//! `arbor://job-*` / `arbor://ide-detection-done` to the frontend.

use std::path::Path;

use crate::config::app_config;
use crate::error::AppError;
use crate::git::worktree::{self, ProjectType, WorktreeInfo, BUILTIN_IDES};
use crate::ipc::corvus;
use crate::AppState;

// ---------------------------------------------------------------------------
// List / Add / Remove
// ---------------------------------------------------------------------------

#[corvus::handler]
fn list_worktrees(state: &AppState, tab_id: String) -> Result<Vec<WorktreeInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::list_worktrees(repo_path, repo_path)
}

#[corvus::handler]
fn add_worktree(
    state: &AppState,
    tab_id: String,
    dest_path: String,
    branch: String,
    new_branch: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::add_worktree(
        repo_path,
        &dest_path,
        &branch,
        new_branch.as_deref(),
    )
}

#[corvus::handler]
fn remove_worktree(
    state: &AppState,
    tab_id: String,
    worktree_path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let repo_path = Path::new(&repo.path);
    worktree::remove_worktree(repo_path, &worktree_path)
}

// ---------------------------------------------------------------------------
// Detect project type (standalone, no repo required)
// ---------------------------------------------------------------------------

// `detect_project_type` took no `State` as a Tauri command, but the broker's
// handler macro requires a context first arg, so we accept `&AppState` and
// ignore it — the decoded JSON args (`path`) are unchanged, so the FE call is
// byte-identical.
#[corvus::handler]
fn detect_project_type(_state: &AppState, path: String) -> Result<ProjectType, AppError> {
    let p = Path::new(&path);
    if !p.exists() {
        return Err(AppError::Other(format!("Path does not exist: {path}")));
    }
    Ok(worktree::detect_project_type(p))
}

// ---------------------------------------------------------------------------
// Open in IDE
// ---------------------------------------------------------------------------

#[corvus::handler]
fn open_in_ide(
    state: &AppState,
    path: String,
    ide_id: Option<String>,
) -> Result<(), AppError> {
    let config = state.lock_config()?;
    let ide_cfg = config.ide.clone();
    drop(config);

    // Per-repo override (Settings → Project → External Integrations) wins
    // over the global default when the caller didn't pin a specific IDE.
    // Best-effort: a missing/unreadable `.arbor/config.toml` just falls
    // through to the global default, the original behavior.
    let repo_ide_id: Option<String> = if ide_id.is_none() {
        crate::config::repo_config::load(&path).ok().and_then(|c| c.ide_id)
    } else {
        None
    };

    let effective_id = ide_id.as_deref()
        .or(repo_ide_id.as_deref())
        .unwrap_or(&ide_cfg.default_ide)
        .to_owned();
    let (command, extra_args) = resolve_ide(&effective_id, &ide_cfg)?;
    worktree::open_in_ide(&path, &command, &extra_args)
}

// ---------------------------------------------------------------------------
// Per-repo IDE preference (`.arbor/config.toml` → `ide_id`)
// ---------------------------------------------------------------------------

/// Read the project-bound IDE preference, or `None` when the repo defers
/// to the global default. Convenience wrapper over `get_repo_config` so
/// the Settings panel doesn't have to round-trip the whole RepoConfig.
#[corvus::handler]
fn get_repo_ide(
    state:  &AppState,
    tab_id: String,
) -> Result<Option<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    Ok(crate::config::repo_config::load(&repo.path)?.ide_id)
}

/// Persist (or clear) the project-bound IDE preference. Pass `None` to
/// remove the override and fall back to the global default.
#[corvus::handler]
fn set_repo_ide(
    state:  &AppState,
    tab_id: String,
    ide_id: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let mut cfg = crate::config::repo_config::load(&repo.path).unwrap_or_default();
    cfg.ide_id = ide_id.filter(|s| !s.is_empty());
    crate::config::repo_config::save(&repo.path, &cfg)
}

// ---------------------------------------------------------------------------
// IDE config get/set
// ---------------------------------------------------------------------------

#[corvus::handler]
fn get_ide_config(
    state: &AppState,
) -> Result<crate::config::app_config::IdeConfig, AppError> {
    let config = state.lock_config()?;
    Ok(config.ide.clone())
}

#[corvus::handler]
fn set_ide_config(
    state: &AppState,
    config: crate::config::app_config::IdeConfig,
) -> Result<(), AppError> {
    let mut cfg = state.lock_config()?;
    cfg.ide = config;
    let cfg_clone = cfg.clone();
    drop(cfg);
    app_config::save(&cfg_clone).map_err(|e| AppError::Other(e.to_string()))
}

// ---------------------------------------------------------------------------
// Helper: resolve IDE command + args from config
// ---------------------------------------------------------------------------

fn resolve_ide(
    ide_id: &str,
    ide_cfg: &crate::config::app_config::IdeConfig,
) -> Result<(String, Vec<String>), AppError> {
    // 1. Custom user-defined IDEs
    if let Some(custom) = ide_cfg.custom_ides.iter().find(|c| c.id == ide_id) {
        return Ok((custom.command.clone(), custom.args.clone()));
    }

    // 2. Built-in IDE — check for a path override first
    if let Some(builtin) = BUILTIN_IDES.iter().find(|b| b.id == ide_id) {
        let cmd = if let Some(ov) = ide_cfg.path_overrides.get(ide_id) {
            if !ov.is_empty() { ov.clone() } else { builtin.cmd.to_owned() }
        } else {
            builtin.cmd.to_owned()
        };
        let args = builtin.args.iter().map(|s| s.to_string()).collect();
        return Ok((cmd, args));
    }

    Err(AppError::Other(format!("Unknown IDE '{ide_id}'")))
}
