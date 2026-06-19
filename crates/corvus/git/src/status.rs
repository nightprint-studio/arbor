//! `status` domain — pure git2 working-tree status, Tauri-free.
//!
//! Lifted verbatim from the shell `crate::git::status`; it was already
//! 100% libgit2 (no `GitCli`, no shell globals), so the extraction is a
//! straight move. `AppError` becomes the crate's [`GitError`] (whose
//! `Display` strings match byte-for-byte, so the wire strings are unchanged).

use git2::{Repository, StatusOptions};
use serde::{Deserialize, Serialize};

use crate::error::Result;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Untracked,
    Ignored,
    Conflicted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub index_status: Option<FileStatus>,
    pub workdir_status: Option<FileStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub current_branch: Option<String>,
    pub head_oid: Option<String>,
    pub is_detached: bool,
    pub ahead: usize,
    pub behind: usize,
    pub staged: Vec<StatusEntry>,
    pub unstaged: Vec<StatusEntry>,
    pub untracked: Vec<StatusEntry>,
    pub conflicted: Vec<StatusEntry>,
    pub is_rebasing: bool,
    pub is_merging: bool,
    pub is_cherry_picking: bool,
    pub is_reverting: bool,
}

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn get_status(repo: &Repository) -> Result<RepoStatus> {
    get_status_with(repo, true)
}

/// Variant that lets callers opt out of rename/copy detection.  Rename detection
/// is O(n²) in libgit2 and becomes the dominant cost on repos with thousands
/// of changed files — exposing the flag lets the command layer honor the
/// user's `status.detect_renames` config setting.
pub fn get_status_with(repo: &Repository, detect_renames: bool) -> Result<RepoStatus> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(detect_renames)
        .renames_index_to_workdir(detect_renames);

    let statuses = repo.statuses(Some(&mut opts))?;

    let mut staged: Vec<StatusEntry> = Vec::new();
    let mut unstaged: Vec<StatusEntry> = Vec::new();
    let mut untracked: Vec<StatusEntry> = Vec::new();
    let mut conflicted: Vec<StatusEntry> = Vec::new();

    for entry in statuses.iter() {
        let s = entry.status();
        let path = entry.path().unwrap_or("").to_string();
        let old_path = entry
            .head_to_index()
            .and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().to_string()))
            .or_else(|| {
                entry
                    .index_to_workdir()
                    .and_then(|d| d.old_file().path().map(|p| p.to_string_lossy().to_string()))
            });

        // Conflicted
        if s.contains(git2::Status::CONFLICTED) {
            conflicted.push(StatusEntry {
                path,
                old_path,
                index_status: Some(FileStatus::Conflicted),
                workdir_status: Some(FileStatus::Conflicted),
            });
            continue;
        }

        // Untracked
        if s.contains(git2::Status::WT_NEW) && !s.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED,
        ) {
            untracked.push(StatusEntry {
                path,
                old_path: None,
                index_status: None,
                workdir_status: Some(FileStatus::Untracked),
            });
            continue;
        }

        // Index (staged)
        let index_status = index_status_from_flags(s);
        if let Some(st) = &index_status {
            let op = if matches!(st, FileStatus::Renamed) { old_path.clone() } else { None };
            staged.push(StatusEntry {
                path: path.clone(),
                old_path: op,
                index_status: index_status.clone(),
                workdir_status: None,
            });
        }

        // Workdir (unstaged)
        let workdir_status = workdir_status_from_flags(s);
        if let Some(st) = &workdir_status {
            let op = if matches!(st, FileStatus::Renamed) { old_path.clone() } else { None };
            unstaged.push(StatusEntry {
                path: path.clone(),
                old_path: op,
                index_status: None,
                workdir_status: workdir_status.clone(),
            });
        }
    }

    let (current_branch, is_detached, head_oid) = match repo.head() {
        Ok(head) => {
            let oid = head.target().map(|id| id.to_string());
            let branch = head.shorthand().map(String::from);
            let detached = repo.head_detached().unwrap_or(false);
            (branch, detached, oid)
        }
        Err(_) => (None, false, None),
    };

    // ahead/behind vs upstream
    let (ahead, behind) = compute_ahead_behind(repo).unwrap_or((0, 0));

    // Detect in-progress operations the same way `git` does — by checking
    // marker files under `.git/`. libgit2's `repo.state()` has been
    // unreliable here (reports `None` even when MERGE_HEAD exists), which
    // meant the MERGING badge never lit up mid-merge.
    let git_dir = repo.path();
    let is_merging = git_dir.join("MERGE_HEAD").exists();
    let is_cherry_picking = git_dir.join("CHERRY_PICK_HEAD").exists();
    let is_reverting = git_dir.join("REVERT_HEAD").exists();
    let is_rebasing = git_dir.join("rebase-merge").is_dir()
        || git_dir.join("rebase-apply").is_dir();

    Ok(RepoStatus {
        current_branch,
        head_oid,
        is_detached,
        ahead,
        behind,
        staged,
        unstaged,
        untracked,
        conflicted,
        is_rebasing,
        is_merging,
        is_cherry_picking,
        is_reverting,
    })
}

fn index_status_from_flags(s: git2::Status) -> Option<FileStatus> {
    if s.contains(git2::Status::INDEX_NEW) {
        Some(FileStatus::Added)
    } else if s.contains(git2::Status::INDEX_MODIFIED) {
        Some(FileStatus::Modified)
    } else if s.contains(git2::Status::INDEX_DELETED) {
        Some(FileStatus::Deleted)
    } else if s.contains(git2::Status::INDEX_RENAMED) {
        Some(FileStatus::Renamed)
    } else {
        None
    }
}

fn workdir_status_from_flags(s: git2::Status) -> Option<FileStatus> {
    if s.contains(git2::Status::WT_MODIFIED) {
        Some(FileStatus::Modified)
    } else if s.contains(git2::Status::WT_DELETED) {
        Some(FileStatus::Deleted)
    } else if s.contains(git2::Status::WT_RENAMED) {
        Some(FileStatus::Renamed)
    } else {
        None
    }
}

fn compute_ahead_behind(repo: &Repository) -> Option<(usize, usize)> {
    let head = repo.head().ok()?;
    let local_oid = head.target()?;
    let branch_name = head.shorthand()?;
    let upstream_ref = format!("refs/remotes/origin/{branch_name}");
    let upstream_oid = repo.refname_to_id(&upstream_ref).ok()?;
    repo.graph_ahead_behind(local_oid, upstream_oid).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Pure flag-mapping invariants — no repo needed.

    #[test]
    fn index_status_priority_new_over_modified() {
        // When both INDEX_NEW and INDEX_MODIFIED are set, "Added" wins
        // (it is checked first) — staging a fresh file then editing it again
        // must still read as Added, not Modified.
        let s = git2::Status::INDEX_NEW | git2::Status::INDEX_MODIFIED;
        assert_eq!(index_status_from_flags(s), Some(FileStatus::Added));
    }

    #[test]
    fn index_status_maps_each_flag() {
        assert_eq!(index_status_from_flags(git2::Status::INDEX_MODIFIED), Some(FileStatus::Modified));
        assert_eq!(index_status_from_flags(git2::Status::INDEX_DELETED), Some(FileStatus::Deleted));
        assert_eq!(index_status_from_flags(git2::Status::INDEX_RENAMED), Some(FileStatus::Renamed));
    }

    #[test]
    fn index_status_none_for_workdir_only_flags() {
        // A purely working-tree change carries no index status.
        assert_eq!(index_status_from_flags(git2::Status::WT_MODIFIED), None);
    }

    #[test]
    fn workdir_status_maps_each_flag() {
        assert_eq!(workdir_status_from_flags(git2::Status::WT_MODIFIED), Some(FileStatus::Modified));
        assert_eq!(workdir_status_from_flags(git2::Status::WT_DELETED), Some(FileStatus::Deleted));
        assert_eq!(workdir_status_from_flags(git2::Status::WT_RENAMED), Some(FileStatus::Renamed));
    }

    #[test]
    fn workdir_status_none_for_index_only_flags() {
        assert_eq!(workdir_status_from_flags(git2::Status::INDEX_NEW), None);
    }

    #[test]
    fn file_status_round_trips_snake_case() {
        // The wire contract is snake_case — guard against an accidental
        // rename_all drift that would break the frontend's status badges.
        let json = serde_json::to_string(&FileStatus::Untracked).unwrap();
        assert_eq!(json, "\"untracked\"");
        let back: FileStatus = serde_json::from_str("\"conflicted\"").unwrap();
        assert_eq!(back, FileStatus::Conflicted);
    }
}
