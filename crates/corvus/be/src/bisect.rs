//! `bisect` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set as the shell's in-process copy, but the context is
//! [`CorvusState`] (not the shell's `AppState`): the repo path comes from the
//! shell-pushed registry ([`CorvusState::repo_path`]) and the git program from
//! `corvus-git-cli` (self-detected). The git logic is the shared [`corvus_git`]
//! crate, so behavior — and error strings — are identical to in-process. Errors
//! cross as their `Display` string (`GitError` → the same text the shell maps to
//! `AppError`).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{BisectMark, BisectSession, BisectState};

use crate::repo::{git, repo_path};

#[arbor_rpc::handler]
fn bisect_start(state: &CorvusState, tab_id: String) -> Result<BisectState, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::bisect_start(&git(state), &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn bisect_mark(state: &CorvusState, tab_id: String, hash: String, mark: String) -> Result<BisectState, String> {
    let mark = match mark.as_str() {
        "good" => BisectMark::Good,
        "bad" => BisectMark::Bad,
        "skip" => BisectMark::Skip,
        other => return Err(format!("unknown bisect mark: {other}")),
    };
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::bisect_mark(&git(state), &path, &hash, mark).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn bisect_reset(state: &CorvusState, tab_id: String) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::bisect_reset(&git(state), &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_bisect_state(state: &CorvusState, tab_id: String) -> Result<BisectState, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::get_bisect_state(&path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn bisect_undo_last_mark(state: &CorvusState, tab_id: String) -> Result<BisectState, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::bisect_undo_last_mark(&git(state), &path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn list_bisect_sessions(state: &CorvusState, tab_id: String) -> Result<Vec<BisectSession>, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::list_sessions(&path).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn save_bisect_session(
    state: &CorvusState,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    name: Option<String>,
) -> Result<BisectSession, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::save_and_pause(&git(state), &path, bad_hashes, good_hashes, name)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn save_bisect_result(
    state: &CorvusState,
    tab_id: String,
    bad_hashes: Vec<String>,
    good_hashes: Vec<String>,
    result_hash: String,
    result_message: Option<String>,
) -> Result<BisectSession, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::save_result(&path, bad_hashes, good_hashes, result_hash, result_message)
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn resume_bisect_session(state: &CorvusState, tab_id: String, session_id: String) -> Result<BisectState, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::resume_session(&git(state), &path, &session_id).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn rename_bisect_session(
    state: &CorvusState,
    tab_id: String,
    session_id: String,
    new_name: String,
) -> Result<BisectSession, String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::rename_session(&path, &session_id, new_name).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn delete_bisect_session(state: &CorvusState, tab_id: String, session_id: String) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    corvus_git::prelude::delete_session(&path, &session_id).map_err(|e| e.to_string())
}
