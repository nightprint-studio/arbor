//! Git awareness for the built-in File Explorer.
//!
//! Three concerns, all path-based (the explorer browses arbitrary filesystem
//! paths, not tab-bound repos like the rest of the app):
//!
//! 1. **Status overlays** — [`fs_git_status`] discovers the enclosing repo and
//!    returns per-entry badges (modified / added / untracked / …) for the items
//!    of one directory, with subfolders rolled up to the strongest descendant
//!    state. Cheaper than TortoiseGit's COM overlay handler and unbounded by the
//!    OS ~15-overlay-slot limit, because the explorer renders every row itself.
//!    Results are cached per repo-root and reused while navigating within the
//!    same repo; the FE passes `refresh = true` (driven by the fs watcher) to
//!    bust the cache.
//! 2. **Inline light actions** — [`fs_git_stage`] / [`fs_git_unstage`] /
//!    [`fs_git_discard`] / [`fs_git_ignore`], operating on absolute paths.
//! 3. **Heavy-action delegation** — [`fs_open_in_arbor`] brings the main window
//!    forward and opens the repo there, reusing Arbor's full git UI (diff / log /
//!    blame / commit) rather than reimplementing it inside the explorer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use git2::{build::CheckoutBuilder, BranchType, IndexAddOption, Repository, Status, StatusOptions};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Types
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

impl GitBadge {
    /// Higher = more important. Used when rolling several descendant states up
    /// to a parent folder: the folder shows the strongest state beneath it.
    fn rank(self) -> u8 {
        match self {
            GitBadge::Conflicted => 6,
            GitBadge::Modified => 5,
            GitBadge::Deleted => 4,
            GitBadge::Renamed => 3,
            GitBadge::Added => 2,
            GitBadge::Untracked => 1,
            GitBadge::Ignored => 0,
        }
    }
}

/// Marker for an immediate child folder that is itself a git repo root — used
/// to flag projects when browsing a folder that *contains* repos (and isn't one
/// itself), TortoiseGit-style. Deliberately lightweight (just HEAD, no status
/// walk and no ahead/behind) so scanning a folder full of repos stays cheap.
#[derive(Clone, Serialize)]
pub struct RepoMarker {
    pub branch: Option<String>,
    pub detached: bool,
}

#[derive(Clone, Serialize)]
pub struct FsGitStatus {
    /// Whether `dir` is inside a git repo. When false the explorer hides every
    /// git affordance for that directory.
    pub in_repo: bool,
    /// Repo working-directory root (native separators), if `in_repo`.
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub ahead: usize,
    pub behind: usize,
    /// Map: normalized child path → overlay badge. Keys are forward-slash,
    /// trailing-slash-stripped, lowercased — matching the FE's `normPath()` so a
    /// row looks up its badge with `badges[normPath(entry.path)]`. Only non-clean
    /// entries are present; everything else renders with no overlay.
    pub badges: HashMap<String, GitBadge>,
    /// Map: normalized child folder path → marker, for immediate children of
    /// `dir` that are themselves git repo roots. Populated regardless of
    /// `in_repo`, so the explorer can flag sibling/nested projects (e.g. a repo
    /// sitting inside a plain parent folder). Same key normalization as `badges`.
    pub repos: HashMap<String, RepoMarker>,
}

// ---------------------------------------------------------------------------
// Path normalization — must mirror the FE `normPath()` exactly
// ---------------------------------------------------------------------------

/// Canonical key for path matching: backslashes → slashes, trailing slashes
/// stripped, lowercased. Mirrors `normPath()` in `FileExplorerModal.svelte`, so
/// backend badge keys line up with `normPath(entry.path)` on the FE. Lowercasing
/// is unconditional (the explorer already assumes case-insensitive matching).
fn norm_key(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

// ---------------------------------------------------------------------------
// Per-repo status cache
// ---------------------------------------------------------------------------

struct RepoStatusCache {
    /// (normalized absolute path, badge) for every non-clean entry in the repo.
    files: Vec<(String, GitBadge)>,
    branch: Option<String>,
    detached: bool,
    ahead: usize,
    behind: usize,
}

fn cache() -> &'static Mutex<HashMap<String, RepoStatusCache>> {
    static C: OnceLock<Mutex<HashMap<String, RepoStatusCache>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// Status mapping
// ---------------------------------------------------------------------------

fn badge_from(s: Status) -> Option<GitBadge> {
    if s.contains(Status::CONFLICTED) {
        return Some(GitBadge::Conflicted);
    }
    // New: staged add → Added; otherwise an untracked working-tree file.
    if s.intersects(Status::WT_NEW | Status::INDEX_NEW) {
        return Some(if s.contains(Status::INDEX_NEW) {
            GitBadge::Added
        } else {
            GitBadge::Untracked
        });
    }
    if s.intersects(Status::WT_MODIFIED | Status::INDEX_MODIFIED) {
        return Some(GitBadge::Modified);
    }
    if s.intersects(Status::WT_DELETED | Status::INDEX_DELETED) {
        return Some(GitBadge::Deleted);
    }
    if s.intersects(Status::WT_RENAMED | Status::INDEX_RENAMED) {
        return Some(GitBadge::Renamed);
    }
    if s.contains(Status::IGNORED) {
        return Some(GitBadge::Ignored);
    }
    None
}

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

/// Compute the full non-clean status of `repo`, keyed by normalized absolute
/// path. Untracked and ignored directories are reported as single entries
/// (`recurse_*_dirs(false)`) — so a `node_modules/` gets one folder badge
/// instead of thousands of file entries, keeping huge repos cheap.
fn compute_repo_status(repo: &Repository) -> RepoStatusCache {
    let workdir = repo.workdir().map(|p| p.to_path_buf());
    let mut files: Vec<(String, GitBadge)> = Vec::new();

    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(true)
        .recurse_ignored_dirs(false)
        // Rename detection is O(n²) in libgit2 and not worth it for overlays —
        // a renamed file simply shows as Added + Deleted, which is fine here.
        .renames_head_to_index(false)
        .renames_index_to_workdir(false);

    if let (Some(wd), Ok(statuses)) = (workdir.as_ref(), repo.statuses(Some(&mut opts))) {
        for entry in statuses.iter() {
            let Some(rel) = entry.path() else { continue };
            let Some(badge) = badge_from(entry.status()) else { continue };
            let abs = wd.join(rel);
            files.push((norm_key(&abs.to_string_lossy()), badge));
        }
    }

    let (branch, detached, ahead, behind) = branch_info(repo);
    RepoStatusCache { files, branch, detached, ahead, behind }
}

/// Slice the repo-wide status down to one directory's children: a file directly
/// in `dir` carries its own badge; anything deeper rolls up to the immediate
/// child folder with the strongest descendant state.
fn slice_for_dir(c: &RepoStatusCache, dir_key: &str) -> HashMap<String, GitBadge> {
    let prefix = format!("{dir_key}/");
    let mut out: HashMap<String, GitBadge> = HashMap::new();
    for (path_key, badge) in &c.files {
        let Some(rest) = path_key.strip_prefix(&prefix) else { continue };
        let Some(child_seg) = rest.split('/').next().filter(|s| !s.is_empty()) else { continue };
        let child_key = format!("{dir_key}/{child_seg}");
        out.entry(child_key)
            .and_modify(|b| { if badge.rank() > b.rank() { *b = *badge; } })
            .or_insert(*badge);
    }
    out
}

/// Read just the current branch / detached flag for a repo, skipping the
/// (potentially expensive) ahead/behind revwalk and status scan. Used for the
/// lightweight child-repo markers.
fn light_head(repo: &Repository) -> (Option<String>, bool) {
    match repo.head() {
        Ok(h) => (h.shorthand().map(String::from), repo.head_detached().unwrap_or(false)),
        Err(_) => (None, false),
    }
}

/// Scan the *immediate* children of `dir` for folders that are themselves git
/// repo roots (have a `.git` entry — directory for normal repos, file for
/// linked worktrees / submodules). One readdir + a stat per subfolder; a repo
/// is opened only for the actual hits, so a plain folder costs almost nothing.
fn scan_child_repos(dir: &str) -> HashMap<String, RepoMarker> {
    let mut out = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return out };
    for ent in rd.flatten() {
        let p = ent.path();
        if !p.is_dir() {
            continue;
        }
        if !p.join(".git").exists() {
            continue;
        }
        let (branch, detached) = match Repository::open(&p) {
            Ok(repo) => light_head(&repo),
            Err(_) => (None, false),
        };
        out.insert(norm_key(&p.to_string_lossy()), RepoMarker { branch, detached });
    }
    out
}

// ---------------------------------------------------------------------------
// Commands — status
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn fs_git_status(dir: String, refresh: Option<bool>) -> Result<FsGitStatus, AppError> {
    let refresh = refresh.unwrap_or(false);
    tokio::task::spawn_blocking(move || {
        // Always flag child repos, even when `dir` itself isn't in a repo — that
        // is exactly the "folder of projects" case the markers exist for.
        let repos = scan_child_repos(&dir);

        let repo = match Repository::discover(&dir) {
            Ok(r) => r,
            Err(_) => return Ok(not_in_repo(repos)),
        };
        let Some(root) = repo.workdir().map(|p| p.to_path_buf()) else {
            return Ok(not_in_repo(repos)); // bare repo — nothing to overlay
        };
        let root_key = norm_key(&root.to_string_lossy());

        // Reuse the cached status while navigating within the same repo; the FE
        // passes refresh=true off the fs watcher to recompute after edits.
        let mut guard = cache()
            .lock()
            .map_err(|_| AppError::Other("status cache poisoned".into()))?;
        if refresh || !guard.contains_key(&root_key) {
            guard.insert(root_key.clone(), compute_repo_status(&repo));
        }
        let c = guard.get(&root_key).expect("just inserted");
        let dir_key = norm_key(&dir);
        Ok(FsGitStatus {
            in_repo: true,
            repo_root: Some(root.to_string_lossy().trim_end_matches(|ch| ch == '/' || ch == '\\').to_string()),
            branch: c.branch.clone(),
            detached: c.detached,
            ahead: c.ahead,
            behind: c.behind,
            badges: slice_for_dir(c, &dir_key),
            repos,
        })
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_status task panicked: {e}")))?
}

fn not_in_repo(repos: HashMap<String, RepoMarker>) -> FsGitStatus {
    FsGitStatus {
        in_repo: false,
        repo_root: None,
        branch: None,
        detached: false,
        ahead: 0,
        behind: 0,
        badges: HashMap::new(),
        repos,
    }
}

// ---------------------------------------------------------------------------
// Command — changes list (staged / unstaged, à la "Check for modifications")
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

#[tauri::command]
pub async fn fs_git_changes(dir: String) -> Result<GitChanges, AppError> {
    tokio::task::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_changes task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Commands — inline light actions (stage / unstage / discard / ignore)
// ---------------------------------------------------------------------------

/// Open the repo enclosing the first path and compute each path relative to its
/// working directory. Returns `(repo, workdir, rel_paths)`. Errors if the paths
/// aren't inside a repo.
fn open_repo_and_rels(paths: &[String]) -> Result<(Repository, PathBuf, Vec<String>), AppError> {
    let first = paths.first().ok_or_else(|| AppError::Other("no paths".into()))?;
    let repo = Repository::discover(first)
        .map_err(|_| AppError::Other("not inside a git repository".into()))?;
    let wd = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_path_buf();
    let wd_key = norm_key(&wd.to_string_lossy());
    let rels: Vec<String> = paths
        .iter()
        .filter_map(|p| {
            norm_key(p)
                .strip_prefix(&wd_key)
                .map(|s| s.trim_start_matches('/').to_string())
                .filter(|s| !s.is_empty())
        })
        .collect();
    Ok((repo, wd, rels))
}

/// Bust the cached status for the repo containing `path` (after a mutating
/// action) so the next `fs_git_status` recomputes even without `refresh`.
fn invalidate_cache(repo: &Repository) {
    if let Some(wd) = repo.workdir() {
        let key = norm_key(&wd.to_string_lossy());
        if let Ok(mut guard) = cache().lock() {
            guard.remove(&key);
        }
    }
}

#[tauri::command]
pub async fn fs_git_stage(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let (repo, _wd, rels) = open_repo_and_rels(&paths)?;
        if rels.is_empty() {
            return Ok(());
        }
        let mut index = repo.index()?;
        // add_all with per-entry pathspecs handles files, whole folders, and
        // deletions (it updates the index to match the worktree) in one pass.
        index.add_all(rels.iter().map(|s| s.as_str()), IndexAddOption::DEFAULT, None)?;
        index.write()?;
        invalidate_cache(&repo);
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_stage task panicked: {e}")))?
}

#[tauri::command]
pub async fn fs_git_unstage(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let (repo, _wd, rels) = open_repo_and_rels(&paths)?;
        if rels.is_empty() {
            return Ok(());
        }
        // revparse_single avoids the peel_to_commit libgit2 bug (see unstage_file).
        match repo.revparse_single("HEAD") {
            Ok(head) => {
                repo.reset_default(Some(&head), rels.iter().map(|s| s.as_str()))
                    .map_err(|e| AppError::Other(format!("unstage: {e}")))?;
            }
            Err(_) => {
                // Pre-initial-commit: nothing committed yet → drop from the index.
                let mut index = repo.index()?;
                for rel in &rels {
                    let _ = index.remove_path(Path::new(rel));
                }
                index.write()?;
            }
        }
        invalidate_cache(&repo);
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_unstage task panicked: {e}")))?
}

#[tauri::command]
pub async fn fs_git_discard(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let (repo, wd, rels) = open_repo_and_rels(&paths)?;
        if rels.is_empty() {
            return Ok(());
        }

        // Safety net: snapshot the workdir so an over-eager discard from the
        // explorer can be undone from Arbor's Recovery tab.
        crate::git::recovery::try_snapshot(
            &repo,
            crate::git::recovery::RecoveryKind::Discard,
            format!("discard {} item(s) from File Explorer", rels.len()),
        );

        let mut checkout = CheckoutBuilder::new();
        let mut any_tracked = false;
        for rel in &rels {
            let status = repo.status_file(Path::new(rel)).unwrap_or(Status::empty());
            if status.intersects(Status::WT_NEW) {
                // Untracked — remove from disk.
                let abs = wd.join(rel);
                if abs.is_dir() {
                    let _ = std::fs::remove_dir_all(&abs);
                } else if abs.exists() {
                    let _ = std::fs::remove_file(&abs);
                }
            } else {
                checkout.path(rel);
                any_tracked = true;
            }
        }
        if any_tracked {
            checkout.force();
            repo.checkout_index(None, Some(&mut checkout))?;
        }
        invalidate_cache(&repo);
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_discard task panicked: {e}")))?
}

#[tauri::command]
pub async fn fs_git_ignore(paths: Vec<String>) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let (repo, wd, rels) = open_repo_and_rels(&paths)?;
        if rels.is_empty() {
            return Ok(());
        }
        let gitignore = wd.join(".gitignore");
        let existing = std::fs::read_to_string(&gitignore).unwrap_or_default();
        let already: std::collections::HashSet<&str> =
            existing.lines().map(|l| l.trim()).collect();

        let mut to_add: Vec<String> = Vec::new();
        for rel in &rels {
            // Anchor to the repo root with a leading slash; folders get a
            // trailing slash so the pattern only matches directories.
            let abs = wd.join(rel);
            let pat = if abs.is_dir() {
                format!("/{}/", rel.trim_end_matches('/'))
            } else {
                format!("/{rel}")
            };
            if !already.contains(pat.as_str()) && !to_add.contains(&pat) {
                to_add.push(pat);
            }
        }
        if to_add.is_empty() {
            return Ok(());
        }

        let mut out = existing;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        for pat in &to_add {
            out.push_str(pat);
            out.push('\n');
        }
        std::fs::write(&gitignore, out).map_err(|e| AppError::Other(e.to_string()))?;
        invalidate_cache(&repo);
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_ignore task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Commands — branch list + switch (checkout)
// ---------------------------------------------------------------------------

/// One local branch, with a flag for the currently checked-out one.
#[derive(Clone, Serialize)]
pub struct FsBranch {
    pub name: String,
    pub is_head: bool,
}

/// Local branches of the repo enclosing `path`, sorted case-insensitively.
#[tauri::command]
pub async fn fs_git_branches(path: String) -> Result<Vec<FsBranch>, AppError> {
    tokio::task::spawn_blocking(move || {
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
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_branches task panicked: {e}")))?
}

/// Switch the repo enclosing `path` to `branch` (a local branch name). Uses a
/// SAFE checkout — refuses to overwrite uncommitted changes rather than
/// clobbering them, surfacing a clear error the explorer turns into a toast.
#[tauri::command]
pub async fn fs_git_checkout(path: String, branch: String) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        let repo = Repository::discover(&path)
            .map_err(|_| AppError::Other("not inside a git repository".into()))?;
        let (object, reference) = repo
            .revparse_ext(&branch)
            .map_err(|e| AppError::Other(format!("unknown branch '{branch}': {e}")))?;
        repo.checkout_tree(&object, None).map_err(|e| {
            AppError::Other(format!(
                "checkout failed — commit or stash your changes first ({e})"
            ))
        })?;
        match reference {
            Some(r) => {
                let name = r
                    .name()
                    .ok_or_else(|| AppError::Other("invalid ref name".into()))?;
                repo.set_head(name)?;
            }
            None => repo.set_head_detached(object.id())?,
        }
        invalidate_cache(&repo);
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("fs_git_checkout task panicked: {e}")))?
}

// ---------------------------------------------------------------------------
// Command — heavy-action delegation ("Open in Arbor")
// ---------------------------------------------------------------------------

/// Bring the main Arbor window forward and ask it to open the repo containing
/// `path`. The heavy git operations (diff / log / blame / commit) live in the
/// main window's full git UI; the explorer just delegates to it.
#[tauri::command]
pub fn fs_open_in_arbor(app: AppHandle, path: String) -> Result<(), AppError> {
    let repo = Repository::discover(&path)
        .map_err(|_| AppError::Other("not inside a git repository".into()))?;
    let root = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_string_lossy()
        .trim_end_matches(|c| c == '/' || c == '\\')
        .to_string();

    // Window focus must happen on the main/UI thread (WebView2 constraint).
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            // Targeted emit (this window only) — the explorer window must not
            // react to its own delegation request.
            let _ = w.emit("arbor://explorer-open-repo", serde_json::json!({ "repoRoot": root }));
        }
    });
    Ok(())
}
