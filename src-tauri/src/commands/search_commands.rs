use tauri::State;

use crate::error::AppError;
use crate::git::search::{SearchQuery, SearchResult};
use crate::AppState;

#[tauri::command]
pub async fn search_commits(
    state: State<'_, AppState>,
    tab_id: String,
    query: SearchQuery,
) -> Result<Vec<SearchResult>, AppError> {
    // Full revwalk + string matching on a large repo can take hundreds of
    // milliseconds.  Run it on the blocking pool so the IPC thread stays
    // responsive to user input while the search is in progress.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::search::search_commits(&repo, &query)
    })
    .await
    .map_err(|e| AppError::Other(format!("search_commits task panicked: {e}")))?
}
