use tauri::State;

use crate::AppState;
use crate::error::AppError;
use crate::git::recovery::{RecoveryEntry, RestorePreview};

/// List all recovery snapshots for a tab (newest first).
#[tauri::command]
pub fn list_recovery_entries(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<RecoveryEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::list_entries(repo.inner())
}

/// Preview what restoring a snapshot would change — file list + dirty check.
#[tauri::command]
pub fn preview_recovery_restore(
    state: State<'_, AppState>,
    tab_id: String,
    entry_id: u64,
) -> Result<RestorePreview, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::preview_restore(repo.inner(), entry_id)
}

/// Restore a snapshot (applies via `git stash apply <snapshot-oid>`).
/// A new recovery snapshot of the current workdir is taken first so the
/// restore itself is reversible.
#[tauri::command]
pub fn restore_recovery_entry(
    state: State<'_, AppState>,
    tab_id: String,
    entry_id: u64,
) -> Result<RecoveryEntry, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::restore(repo.inner(), entry_id)
}

/// Delete a snapshot entry and drop its ref.  The underlying commit may
/// remain reachable from the reflog until `git gc` runs, but it is no longer
/// exposed in the recovery UI.
#[tauri::command]
pub fn delete_recovery_entry(
    state: State<'_, AppState>,
    tab_id: String,
    entry_id: u64,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::delete(repo.inner(), entry_id)
}
