use tauri::State;

use crate::error::AppError;
use crate::git::merge::{self, ConflictContent, ConflictPresence, MergeOutcome, MergeStrategy};
use crate::AppState;

// ---------------------------------------------------------------------------
// Merge conflict resolution commands
// ---------------------------------------------------------------------------

/// Return the three-way content for a conflicted file (ours / theirs / base /
/// working-tree).  Read-only — uses an immutable repo reference.
///
/// `encoding_override` (e.g. `"windows-1252"`) skips auto-detection and
/// forces a specific encoding for all three stages. Frontend persists the
/// user's pick per-file in localStorage and replays it here.
#[tauri::command]
pub fn get_conflict_content(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<ConflictContent, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    merge::get_conflict_content(repo.inner(), &path, encoding_override.as_deref())
}

/// Cheap presence-only query for every conflicted file — drives the
/// "added by them" / "deleted by them" badges in the modal's sidebar
/// without paying the full three-way load for each entry.
#[tauri::command]
pub fn get_conflict_presence(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<ConflictPresence>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    merge::get_conflict_presence(repo.inner())
}

/// Write the resolved content to disk and stage the file so it is no longer
/// listed as conflicted.
///
/// `encoding` is the label returned by `get_conflict_content` — round-trip
/// it back here so legacy windows-1252 / Latin-1 files don't get silently
/// rewritten as UTF-8.
#[tauri::command]
pub fn resolve_conflict(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    merge::resolve_conflict(repo.inner_mut(), &path, &content, encoding.as_deref())
}

/// Resolve a conflict by removing the file entirely (modify/delete or
/// add/modify when the user picks "accept deletion").  Drops the workdir
/// file plus every index stage for that path.
#[tauri::command]
pub fn remove_conflict_file(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    merge::remove_conflict_file(repo.inner_mut(), &path)
}

/// Write the resolved content to disk and reset the index to HEAD (unstaged).
/// Used for stash conflict resolution where staging is not desired.
#[tauri::command]
pub fn resolve_stash_conflict(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    merge::resolve_stash_conflict(repo.inner_mut(), &path, &content, encoding.as_deref())
}

/// Create the merge commit (HEAD + MERGE_HEAD as parents) with the provided
/// commit message, then clean up the merge state.
/// Returns the new commit OID as a hex string.
#[tauri::command]
pub fn complete_merge(
    state: State<'_, AppState>,
    tab_id: String,
    message: String,
) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    merge::complete_merge(repo.inner_mut(), &message)
}

/// Abort the ongoing merge and restore the working tree to its pre-merge state
/// (equivalent to `git merge --abort`).
#[tauri::command]
pub fn abort_merge(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    let workdir = repo
        .inner()
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    drop(mgr); // release the lock before spawning a subprocess
    merge::abort_merge(&workdir)
}

/// Merge `branch_name` into the current HEAD via the git CLI.
///
/// `strategy` selects between default merge, `--no-ff`, `--ff-only` and
/// `--squash`. When omitted, defaults to the standard merge behaviour for
/// backward compatibility with legacy callers.
///
/// Drops the repo lock before spawning the subprocess so libgit2's internal
/// state is not poisoned.  Returns `Err("CONFLICTS:…")` when the merge
/// produces conflicts so the frontend can redirect the user to the conflict
/// resolver without treating it as a hard error.
#[tauri::command]
pub fn merge_branch(
    state: State<'_, AppState>,
    tab_id: String,
    branch_name: String,
    strategy: Option<MergeStrategy>,
) -> Result<MergeOutcome, AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    }; // lock released here
    merge::merge_branch(&workdir, &branch_name, strategy.unwrap_or_default())
}

/// Read the pre-filled merge commit message from `.git/MERGE_MSG`.
#[tauri::command]
pub fn get_merge_message(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<String, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    merge::get_merge_message(repo.inner())
}
