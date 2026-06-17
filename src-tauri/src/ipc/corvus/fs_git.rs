//! `fs_git` domain — cache-free, read-only git queries for the built-in File
//! Explorer, routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline. The
//! commands were `async fn` whose whole body was a single
//! `tokio::task::spawn_blocking(...)`; the broker runs handlers on a blocking
//! thread already, so the wrapper is unwound here and the body runs directly —
//! the blocking git work and its errors are byte-identical. `#[corvus::handler]`
//! self-registers each under its own function name.
//!
//! These three queries never touch `AppState`, but the handler macro requires a
//! context first arg, so they take `_state: &AppState` and ignore it.
//!
//! NOT migrated (stay inline in `fs_git_commands`, handled by a later pass):
//!  * `fs_git_status` — reads/populates a process-global status cache that the
//!    inline mutating actions (`fs_git_stage`/`unstage`/`discard`/`ignore`/
//!    `checkout`) invalidate via `invalidate_cache`. A single `OnceLock` cache
//!    can't be split across modules without becoming two divergent caches, so
//!    status moves only when its mutators do.
//!  * The mutating light actions (`stage`/`unstage`/`discard`/`ignore`/
//!    `checkout`) — they mutate the index/worktree, snapshot to Recovery, and
//!    invalidate the shared cache; not leaf queries.
//!  * `fs_open_in_arbor` — takes an `AppHandle`, focuses the main window on the
//!    UI thread, and emits `arbor://explorer-open-repo`.
//!
//! No hooks fire in this domain.

use git2::{BranchType, Repository, Status, StatusOptions};
use serde::Serialize;

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

// ---------------------------------------------------------------------------
// Shared status badge + branch info (pure; mirrors `fs_git_commands`)
// ---------------------------------------------------------------------------

/// A single overlay badge for a file or (rolled-up) folder.
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GitBadge {
    Conflicted,
    Modified,
    Deleted,
    Renamed,
    Added,
    Untracked,
    Ignored,
}

/// Current branch / detached flag / ahead-behind for a repo's HEAD.
fn branch_info(repo: &Repository) -> (Option<String>, bool, usize, usize) {
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return (None, false, 0, 0),
    };
    let branch = head.shorthand().map(String::from);
    let detached = repo.head_detached().unwrap_or(false);
    let (mut ahead, mut behind) = (0, 0);
    if let (Some(local), Some(name)) = (head.target(), head.shorthand()) {
        let upstream = format!("refs/remotes/origin/{name}");
        if let Ok(up_oid) = repo.refname_to_id(&upstream) {
            if let Ok((a, b)) = repo.graph_ahead_behind(local, up_oid) {
                ahead = a;
                behind = b;
            }
        }
    }
    (branch, detached, ahead, behind)
}

// ---------------------------------------------------------------------------
// Changes list (staged / unstaged, à la "Check for modifications")
// ---------------------------------------------------------------------------

/// One changed file, in either the staged (index) or unstaged (worktree) list.
#[derive(Clone, Serialize)]
pub struct GitChange {
    /// Absolute path with native separators — matches `fsReadDir` entry paths so
    /// the explorer can reveal/select the row.
    pub path: String,
    /// Repo-relative path (forward slashes) for display.
    pub rel: String,
    pub badge: GitBadge,
}

/// Full working-tree change list for the repo enclosing `dir`. A file edited
/// after being staged appears in BOTH lists (its index side staged, its
/// worktree side unstaged) — exactly like `git status`.
#[derive(Clone, Default, Serialize)]
pub struct GitChanges {
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub staged: Vec<GitChange>,
    pub unstaged: Vec<GitChange>,
}

/// Badge for the index (staged) side of a status, if any.
fn index_badge(s: Status) -> Option<GitBadge> {
    if s.contains(Status::INDEX_NEW) {
        Some(GitBadge::Added)
    } else if s.intersects(Status::INDEX_MODIFIED | Status::INDEX_TYPECHANGE) {
        Some(GitBadge::Modified)
    } else if s.contains(Status::INDEX_DELETED) {
        Some(GitBadge::Deleted)
    } else if s.contains(Status::INDEX_RENAMED) {
        Some(GitBadge::Renamed)
    } else {
        None
    }
}

/// Badge for the worktree (unstaged) side of a status, if any. Conflicts are
/// surfaced here so they never get lost in the staged list.
fn worktree_badge(s: Status) -> Option<GitBadge> {
    if s.contains(Status::CONFLICTED) {
        Some(GitBadge::Conflicted)
    } else if s.contains(Status::WT_NEW) {
        Some(GitBadge::Untracked)
    } else if s.intersects(Status::WT_MODIFIED | Status::WT_TYPECHANGE) {
        Some(GitBadge::Modified)
    } else if s.contains(Status::WT_DELETED) {
        Some(GitBadge::Deleted)
    } else if s.contains(Status::WT_RENAMED) {
        Some(GitBadge::Renamed)
    } else {
        None
    }
}

#[corvus::handler]
fn fs_git_changes(_state: &AppState, dir: String) -> Result<GitChanges, AppError> {
    let repo = match Repository::discover(&dir) {
        Ok(r) => r,
        Err(_) => return Ok(GitChanges::default()),
    };
    let Some(wd) = repo.workdir().map(|p| p.to_path_buf()) else {
        return Ok(GitChanges::default());
    };

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    let mut staged: Vec<GitChange> = Vec::new();
    let mut unstaged: Vec<GitChange> = Vec::new();
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        for entry in statuses.iter() {
            let Some(rel) = entry.path() else { continue };
            let s = entry.status();
            let abs = wd.join(rel);
            let path = abs
                .to_string_lossy()
                .trim_end_matches(|c| c == '/' || c == '\\')
                .to_string();
            let rel_disp = rel.replace('\\', "/").trim_end_matches('/').to_string();
            if let Some(badge) = index_badge(s) {
                staged.push(GitChange { path: path.clone(), rel: rel_disp.clone(), badge });
            }
            if let Some(badge) = worktree_badge(s) {
                unstaged.push(GitChange { path, rel: rel_disp, badge });
            }
        }
    }
    staged.sort_by(|a, b| a.rel.cmp(&b.rel));
    unstaged.sort_by(|a, b| a.rel.cmp(&b.rel));

    let (branch, ..) = branch_info(&repo);
    Ok(GitChanges {
        repo_root: Some(
            wd.to_string_lossy()
                .trim_end_matches(|c| c == '/' || c == '\\')
                .to_string(),
        ),
        branch,
        staged,
        unstaged,
    })
}

// ---------------------------------------------------------------------------
// Branch list
// ---------------------------------------------------------------------------

/// One local branch, with a flag for the currently checked-out one.
#[derive(Clone, Serialize)]
pub struct FsBranch {
    pub name: String,
    pub is_head: bool,
}

/// Local branches of the repo enclosing `path`, sorted case-insensitively.
#[corvus::handler]
fn fs_git_branches(_state: &AppState, path: String) -> Result<Vec<FsBranch>, AppError> {
    let repo = Repository::discover(&path)
        .map_err(|_| AppError::Other("not inside a git repository".into()))?;
    let mut out = Vec::new();
    for b in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = b?;
        let is_head = branch.is_head();
        if let Some(name) = branch.name()?.map(String::from) {
            out.push(FsBranch { name, is_head });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Remote URL ("Copy project link")
// ---------------------------------------------------------------------------

/// Resolve the remote URL of the repo enclosing `path` (a file or directory),
/// for the explorer's "Copy project link". Prefers the remote named `origin`,
/// falling back to the first remote. Returns `None` when `path` isn't inside a
/// repo or the repo has no remote — the FE then toasts instead of copying a
/// non-shareable link. `Repository::discover` walks up from any subpath, so the
/// caller can pass the right-clicked entry directly.
#[corvus::handler]
fn fs_git_remote_url(_state: &AppState, path: String) -> Result<Option<String>, AppError> {
    let Ok(repo) = Repository::discover(&path) else { return Ok(None) };
    let remotes = repo.remotes()?;
    let pick = remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next());
    let Some(name) = pick else { return Ok(None) };
    let url = repo
        .find_remote(name)
        .ok()
        .and_then(|r| r.url().map(str::to_string))
        .filter(|u| !u.trim().is_empty());
    Ok(url)
}
