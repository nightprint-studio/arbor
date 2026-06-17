//! `search` domain — handler routed through the in-process broker.
//!
//! The handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! revwalk now lives in [`corvus_git::search`]; this shell layer keeps only the
//! brief repo-path lock + repo reopen, so behavior (which refs are walked, which
//! commits match, the result shape, the errors) is byte-identical.
//!
//! The old command ran the revwalk on `spawn_blocking` to keep the IPC thread
//! responsive. That wrapper is gone here on purpose: the generic `rpc` command
//! already runs the whole dispatch on `tokio::task::spawn_blocking` (see
//! `crate::commands::rpc_commands::rpc`), so the search still executes off the
//! async runtime — the per-handler wrapper would just be a redundant nested
//! blocking task. The brief-lock-then-reopen shape is preserved unchanged.

use crate::error::AppError;
use crate::git::search::{SearchQuery, SearchResult};
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn search_commits(state: &AppState, tab_id: String, query: SearchQuery) -> Result<Vec<SearchResult>, AppError> {
    // Brief-lock `repos` to clone the path, then drop the lock before the
    // revwalk — same as the old command, so libgit2 walks a freshly opened
    // repo and nothing else is held across the (potentially long) search.
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::search::search_commits(&repo, &query)
}
