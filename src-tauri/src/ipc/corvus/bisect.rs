//! `bisect` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. Behavior
//! is byte-identical — only the call path changed.

use crate::error::AppError;
use crate::git::bisect::{BisectMark, BisectState};
use crate::git::bisect_sessions::BisectSession;
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn bisect_start(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_start(&repo.path)
}

#[corvus::handler]
fn bisect_mark(state: &AppState, tab_id: String, hash: String, mark: String) -> Result<BisectState, AppError> {
    let mark = match mark.as_str() {
        "good" => BisectMark::Good,
        "bad" => BisectMark::Bad,
        "skip" => BisectMark::Skip,
        other => return Err(AppError::Other(format!("unknown bisect mark: {other}"))),
    };
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_mark(&repo.path, &hash, mark)
}

#[corvus::handler]
fn bisect_reset(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_reset(&repo.path)
}

#[corvus::handler]
fn get_bisect_state(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::get_bisect_state(&repo.path)
}

#[corvus::handler]
fn bisect_undo_last_mark(state: &AppState, tab_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect::bisect_undo_last_mark(&repo.path)
}

#[corvus::handler]
fn list_bisect_sessions(state: &AppState, tab_id: String) -> Result<Vec<BisectSession>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::list_sessions(&repo.path)
}

#[corvus::handler]
fn save_bisect_session(
    state: &AppState,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    name: Option<String>,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::save_and_pause(&repo.path, bad_hashes, good_hashes, name)
}

#[corvus::handler]
fn save_bisect_result(
    state: &AppState,
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

#[corvus::handler]
fn resume_bisect_session(state: &AppState, tab_id: String, session_id: String) -> Result<BisectState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::resume_session(&repo.path, &session_id)
}

#[corvus::handler]
fn rename_bisect_session(
    state: &AppState,
    tab_id: String,
    session_id: String,
    new_name: String,
) -> Result<BisectSession, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::rename_session(&repo.path, &session_id, new_name)
}

#[corvus::handler]
fn delete_bisect_session(state: &AppState, tab_id: String, session_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::bisect_sessions::delete_session(&repo.path, &session_id)
}
