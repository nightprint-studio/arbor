//! `recovery` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The snapshot/journal logic moved into [`corvus_git::recovery`] (so the
//! headless `corvus-be` shares it). This module keeps the original shell-facing
//! API — same signatures, `AppError` results, the config-loading convenience
//! `snapshot` / `try_snapshot` — so the ~9 in-process consumers (checkout-safe
//! stashing, discard, pull, reset, linked-worktree sync, the recovery IPC
//! handlers) are untouched.
//!
//! Two couplings the crate deliberately does NOT take on are injected here:
//! - **git invocation** → the shell's resolved program as a [`GitCli`];
//! - **snapshot policy / retention** → loaded from the app config and forwarded
//!   (so `corvus-git` drags in neither `git_cli` globals nor the app config).

use git2::Repository;

use corvus_git::prelude::GitCli;

use crate::error::Result;

// Re-export the data types + policy + defaults so existing
// `crate::git::recovery::Recovery*` / `SnapshotPolicy` / `DEFAULT_*` paths
// (config, recovery commands) keep resolving.
pub use corvus_git::prelude::{
    RecoveryEntry, RecoveryKind, RestorePreview, SnapshotPolicy,
    DEFAULT_DENY_EXTENSIONS, DEFAULT_MAX_FILE_SIZE, DEFAULT_RETENTION_DAYS,
};

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> GitCli {
    GitCli::from_optional(crate::git_cli::snapshot().path)
}

/// Load the user-tuned snapshot policy. The `recovery` config section is OWNED
/// by corvus-be now (`corvus/config.toml`); the in-process snapshotter reads it
/// back with a thin partial-struct read. Falls back to the built-in defaults if
/// the file/section is unavailable or malformed.
fn load_policy_from_config() -> SnapshotPolicy {
    crate::config::corvus_read::section::<SnapshotPolicy>("recovery").unwrap_or_default()
}

/// Capture a snapshot under an explicit [`SnapshotPolicy`].
pub fn snapshot_with_policy(
    repo:    &Repository,
    kind:    RecoveryKind,
    summary: impl Into<String>,
    policy:  &SnapshotPolicy,
) -> Result<Option<RecoveryEntry>> {
    Ok(corvus_git::recovery::snapshot_with_policy(&git(), repo, kind, summary, policy)?)
}

/// Capture a snapshot using the app-wide configured policy.
pub fn snapshot(
    repo:    &Repository,
    kind:    RecoveryKind,
    summary: impl Into<String>,
) -> Result<Option<RecoveryEntry>> {
    let policy = load_policy_from_config();
    snapshot_with_policy(repo, kind, summary, &policy)
}

/// Thin wrapper used by commands that want to call snapshot() without taking
/// on the full error handling burden.  Logs and swallows all failures.
pub fn try_snapshot(repo: &Repository, kind: RecoveryKind, summary: impl Into<String>) {
    if let Err(e) = snapshot(repo, kind, summary) {
        tracing::warn!("recovery snapshot skipped: {e}");
    }
}

/// List all known recovery entries, newest first (prunes expired/over-cap).
pub fn list_entries(repo: &Repository) -> Result<Vec<RecoveryEntry>> {
    let policy = load_policy_from_config();
    Ok(corvus_git::recovery::list_entries(&git(), repo, policy.retention_days)?)
}

/// Describe what a restore would do without performing it.
pub fn preview_restore(repo: &Repository, entry_id: u64) -> Result<RestorePreview> {
    let policy = load_policy_from_config();
    Ok(corvus_git::recovery::preview_restore(&git(), repo, entry_id, policy.retention_days)?)
}

/// Restore a snapshot (after self-snapshotting the current state).
pub fn restore(repo: &Repository, entry_id: u64) -> Result<RecoveryEntry> {
    let policy = load_policy_from_config();
    Ok(corvus_git::recovery::restore(&git(), repo, entry_id, &policy)?)
}

/// Delete a snapshot and its journal entry.
pub fn delete(repo: &Repository, entry_id: u64) -> Result<()> {
    Ok(corvus_git::recovery::delete(&git(), repo, entry_id)?)
}
