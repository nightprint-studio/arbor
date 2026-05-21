use tauri::State;

use crate::error::AppError;
use crate::git::bisect::{BisectMark, BisectState};
use crate::git::bisect_sessions::BisectSession;
use crate::AppState;

#[tauri::command]
pub fn bisect_start(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_start(&repo.path)
}

#[tauri::command]
pub fn bisect_mark(
    state: State<'_, AppState>,
    tab_id: String,
    hash: String,
    mark: String,
) -> Result<BisectState, AppError> {
    let mark = match mark.as_str() {
        "good" => BisectMark::Good,
        "bad"  => BisectMark::Bad,
        "skip" => BisectMark::Skip,
        other  => return Err(AppError::Other(format!("unknown bisect mark: {other}"))),
    };
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_mark(&repo.path, &hash, mark)
}

#[tauri::command]
pub fn bisect_reset(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_reset(&repo.path)
}

#[tauri::command]
pub fn get_bisect_state(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::get_bisect_state(&repo.path)
}

#[tauri::command]
pub fn bisect_undo_last_mark(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_undo_last_mark(&repo.path)
}

#[tauri::command]
pub fn list_bisect_sessions(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<BisectSession>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::list_sessions(&repo.path)
}

#[tauri::command]
pub fn save_bisect_session(
    state: State<'_, AppState>,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    name: Option<String>,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::save_and_pause(&repo.path, bad_hashes, good_hashes, name)
}

#[tauri::command]
pub fn save_bisect_result(
    state: State<'_, AppState>,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    result_hash: String,
    result_message: Option<String>,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::save_result(&repo.path, bad_hashes, good_hashes, result_hash, result_message)
}

#[tauri::command]
pub fn resume_bisect_session(
    state: State<'_, AppState>,
    tab_id: String,
    session_id: String,
) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::resume_session(&repo.path, &session_id)
}

#[tauri::command]
pub fn rename_bisect_session(
    state: State<'_, AppState>,
    tab_id: String,
    session_id: String,
    new_name: String,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::rename_session(&repo.path, &session_id, new_name)
}

#[tauri::command]
pub fn delete_bisect_session(
    state: State<'_, AppState>,
    tab_id: String,
    session_id: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::delete_session(&repo.path, &session_id)
}
