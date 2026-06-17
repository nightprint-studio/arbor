//! `recovery` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name** (reading the signature to generate the JSON-arg decode), so the
//! command is reached generically through the router. Behavior (locks held,
//! errors) is byte-identical — only the call path changed.
//!
//! The snapshot/journal git logic already lives in [`corvus_git::recovery`];
//! these handlers delegate to the config-loading shell wrapper
//! [`crate::git::recovery`] (which injects the resolved `GitCli` + the
//! app-configured `SnapshotPolicy`), so no crate extraction is needed here.

use crate::error::AppError;
use crate::git::recovery::{RecoveryEntry, RestorePreview};
use crate::ipc::corvus;
use crate::AppState;

/// List all recovery snapshots for a tab (newest first).
#[corvus::handler]
fn list_recovery_entries(state: &AppState, tab_id: String) -> Result<Vec<RecoveryEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::list_entries(repo.inner())
}

/// Preview what restoring a snapshot would change — file list + dirty check.
#[corvus::handler]
fn preview_recovery_restore(
    state: &AppState,
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
#[corvus::handler]
fn restore_recovery_entry(
    state: &AppState,
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
#[corvus::handler]
fn delete_recovery_entry(state: &AppState, tab_id: String, entry_id: u64) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::recovery::delete(repo.inner(), entry_id)
}
