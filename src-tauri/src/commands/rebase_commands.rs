use tauri::State;

use crate::error::AppError;
use crate::git::rebase::{RebaseTodoEntry, RebaseState};
use crate::AppState;

#[tauri::command]
pub fn get_rebase_todo(
    state: State<'_, AppState>,
    tab_id: String,
    base: String,
) -> Result<Vec<RebaseTodoEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::rebase::get_rebase_todo(&repo.path, &base)
}

#[tauri::command]
pub fn start_rebase(
    state: State<'_, AppState>,
    tab_id: String,
    base: String,
    todo: Vec<RebaseTodoEntry>,
) -> Result<(), AppError> {
    let action_count = todo.len();
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::rebase::start_interactive_rebase(&repo.path, &base, &todo)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":       &tab_id,
            "base":         &base,
            "action_count": action_count,
        });
        let _ = host.fire_hook("on_rebase_start", &ctx.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn rebase_continue(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::rebase::rebase_continue(&repo.path)
}

#[tauri::command]
pub fn rebase_abort(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::rebase::rebase_abort(&repo.path)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id });
        let _ = host.fire_hook("on_rebase_abort", &ctx.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn rebase_skip(state: State<'_, AppState>, tab_id: String) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::rebase::rebase_skip(&repo.path)
}

#[tauri::command]
pub fn get_rebase_state(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<RebaseState, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let r = repo.inner();
    let git_state = r.state();
    let in_progress = matches!(
        git_state,
        git2::RepositoryState::Rebase
            | git2::RepositoryState::RebaseInteractive
            | git2::RepositoryState::RebaseMerge
    );
    Ok(RebaseState {
        in_progress,
        current_step: 0,
        total_steps: 0,
        conflicted_files: Vec::new(),
    })
}
