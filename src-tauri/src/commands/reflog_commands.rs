use tauri::State;
use crate::AppState;
use crate::error::Result;
use crate::git::reflog::ReflogEntry;

#[tauri::command]
pub async fn get_reflog(
    state: State<'_, AppState>,
    tab_id: String,
    limit: Option<usize>,
) -> Result<Vec<ReflogEntry>> {
    let mut repos = state.lock_repos()?;
    let repo  = repos.get(&tab_id)?;
    crate::git::reflog::get_reflog(repo, limit)
}
