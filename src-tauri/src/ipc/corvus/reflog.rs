//! `reflog` domain — handler routed through the in-process broker.
//!
//! Same body the matching `#[tauri::command]` ran inline; `#[corvus::handler]`
//! self-registers it under its own function name. Behavior is byte-identical —
//! only the call path changed.

use crate::error::Result;
use crate::git::reflog::ReflogEntry;
use crate::ipc::corvus;
use crate::AppState;

#[corvus::handler]
fn get_reflog(state: &AppState, tab_id: String, limit: Option<usize>) -> Result<Vec<ReflogEntry>> {
    let mut repos = state.lock_repos()?;
    let repo = repos.get(&tab_id)?;
    crate::git::reflog::get_reflog(repo, limit)
}
