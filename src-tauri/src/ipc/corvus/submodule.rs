//! `submodule` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name** (reading the signature to generate the JSON-arg decode), so the
//! command is reached generically through the router. Behavior (locks held,
//! errors) is byte-identical — only the call path changed.
//!
//! The git work stays in the reusable shell module [`crate::git::submodule`]:
//! it shells out to git via `crate::git_cli` (host-scoped HTTP auth `-c`
//! pairs) and uses `crate::process_ext::NoWindowExt`, both credential- and
//! shell-coupled. So there is no `corvus-git` extraction for this domain — the
//! handlers delegate to the existing module directly, exactly as the old
//! commands did.
//!
//! No hooks fire for any submodule command.

use crate::error::AppError;
use crate::git::submodule::SubmoduleInfo;
use crate::ipc::corvus;
use crate::AppState;

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

#[corvus::handler]
fn list_submodules(state: &AppState, tab_id: String) -> Result<Vec<SubmoduleInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::list_submodules(repo.inner())
}

// ---------------------------------------------------------------------------
// Per-submodule operations
// ---------------------------------------------------------------------------

#[corvus::handler]
fn submodule_fetch(state: &AppState, tab_id: String, sub_path: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_fetch(repo.inner(), &sub_path)
}

#[corvus::handler]
fn submodule_pull(state: &AppState, tab_id: String, sub_path: String) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_pull(repo.inner(), &sub_path)
}

#[corvus::handler]
fn submodule_push(state: &AppState, tab_id: String, sub_path: String) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_push(repo.inner(), &sub_path)
}

#[corvus::handler]
fn submodule_checkout(
    state: &AppState,
    tab_id: String,
    sub_path: String,
    branch: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_checkout(repo.inner(), &sub_path, &branch)
}

#[corvus::handler]
fn submodule_list_branches(
    state: &AppState,
    tab_id: String,
    sub_path: String,
) -> Result<Vec<String>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_list_branches(repo.inner(), &sub_path)
}

// ---------------------------------------------------------------------------
// Parent-level update helpers (kept for backward compatibility)
// ---------------------------------------------------------------------------

#[corvus::handler]
fn update_submodule(
    state: &AppState,
    tab_id: String,
    name: String,
    recursive: bool,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::update_submodule(&repo.path, &name, recursive)
}

#[corvus::handler]
fn update_all_submodules(
    state: &AppState,
    tab_id: String,
    recursive: bool,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::update_submodules(&repo.path, recursive)
}
