use tauri::State;

use crate::error::AppError;
use crate::git::submodule::SubmoduleInfo;
use crate::AppState;

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_submodules(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<SubmoduleInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::list_submodules(repo.inner())
}

// ---------------------------------------------------------------------------
// Per-submodule operations
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn submodule_fetch(
    state: State<'_, AppState>,
    tab_id: String,
    sub_path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_fetch(repo.inner(), &sub_path)
}

#[tauri::command]
pub fn submodule_pull(
    state: State<'_, AppState>,
    tab_id: String,
    sub_path: String,
) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_pull(repo.inner(), &sub_path)
}

#[tauri::command]
pub fn submodule_push(
    state: State<'_, AppState>,
    tab_id: String,
    sub_path: String,
) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_push(repo.inner(), &sub_path)
}

#[tauri::command]
pub fn submodule_checkout(
    state: State<'_, AppState>,
    tab_id: String,
    sub_path: String,
    branch: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::submodule_checkout(repo.inner(), &sub_path, &branch)
}

#[tauri::command]
pub fn submodule_list_branches(
    state: State<'_, AppState>,
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

#[tauri::command]
pub fn update_submodule(
    state: State<'_, AppState>,
    tab_id: String,
    name: String,
    recursive: bool,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::update_submodule(&repo.path, &name, recursive)
}

#[tauri::command]
pub fn update_all_submodules(
    state: State<'_, AppState>,
    tab_id: String,
    recursive: bool,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::submodule::update_submodules(&repo.path, recursive)
}
