//! `recovery` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (and function names → method names) as the shell's
//! in-process copy (`crate::ipc::corvus::recovery`), but the context is
//! [`CorvusState`] instead of the shell's `AppState`: the repo is opened by the
//! shell-pushed path ([`CorvusState::repo_path`]) and the git program comes from
//! `corvus-git-cli` (self-detected). The snapshot/journal logic is the shared
//! [`corvus_git::recovery`] crate, so the listed entries, the restore preview,
//! the restored state, and the error strings are identical to in-process
//! (`GitError`'s `Display` is the text the shell maps to `AppError`).
//!
//! Read + restore + delete — this domain fires **no hooks** (the in-process copy
//! fires none either).
//!
//! The user-tuned retention/size policy is the one the shell pushes into
//! `CorvusState` (`crate::repo::snapshot_policy`); absent a push it falls back to
//! [`SnapshotPolicy::default`]. So an OOP list/prune/restore honours the same
//! configured limits as in-process (W0b).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{RecoveryEntry, RestorePreview};

use crate::repo::{git, open, snapshot_policy};

/// List all recovery snapshots for a tab (newest first).
///
/// Pruning of expired/over-cap entries happens inside the crate against the
/// policy's `retention_days` — the shell-pushed value, else the default window.
#[arbor_rpc::handler]
fn list_recovery_entries(state: &CorvusState, tab_id: String) -> Result<Vec<RecoveryEntry>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::recovery::list_entries(&git(state), &repo, snapshot_policy(state).retention_days)
        .map_err(|e| e.to_string())
}

/// Preview what restoring a snapshot would change — file list + dirty check.
#[arbor_rpc::handler]
fn preview_recovery_restore(
    state: &CorvusState,
    tab_id: String,
    entry_id: u64,
) -> Result<RestorePreview, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::recovery::preview_restore(
        &git(state),
        &repo,
        entry_id,
        snapshot_policy(state).retention_days,
    )
    .map_err(|e| e.to_string())
}

/// Restore a snapshot (applies via `git stash apply <snapshot-oid>`).
/// A new recovery snapshot of the current workdir is taken first (by the crate)
/// so the restore itself is reversible.
#[arbor_rpc::handler]
fn restore_recovery_entry(
    state: &CorvusState,
    tab_id: String,
    entry_id: u64,
) -> Result<RecoveryEntry, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::recovery::restore(&git(state), &repo, entry_id, &snapshot_policy(state))
        .map_err(|e| e.to_string())
}

/// Delete a snapshot entry and drop its ref. The underlying commit may remain
/// reachable from the reflog until `git gc` runs, but it is no longer exposed
/// in the recovery UI.
#[arbor_rpc::handler]
fn delete_recovery_entry(state: &CorvusState, tab_id: String, entry_id: u64) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    corvus_git::recovery::delete(&git(state), &repo, entry_id).map_err(|e| e.to_string())
}
