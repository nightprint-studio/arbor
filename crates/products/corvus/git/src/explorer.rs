//! `explorer` — git awareness for the built-in File Explorer (overlay badges,
//! branch info, light inline actions), as **pure local git2 logic**.
//!
//! Extracted from the shell/`corvus-be` `fs_git` handlers so the file-explorer
//! product (`sitta-be`) and any other consumer run the exact same code without
//! duplicating it. Every function takes plain paths — the explorer browses
//! arbitrary on-disk paths, NOT tab-bound repos, so nothing here touches a repo
//! registry or any product state. Operations are LOCAL git2 only (no network).
//!
//! The status query ([`status`]) and the inline mutating actions
//! ([`stage`]/[`unstage`]/[`discard`]/[`ignore`]/[`checkout`]) share a
//! process-global per-repo status cache (the [`cache`] `OnceLock`): the actions
//! invalidate it after mutating so the next [`status`] recomputes.
//!
//! [`discard`] optionally takes a `(GitCli, SnapshotPolicy)` so the caller can opt
//! into a recovery snapshot (the safety net behind Arbor's Recovery tab) before
//! the working-tree changes are thrown away — the policy/git invoker are injected
//! because they are product config, not an explorer concern.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use git2::{build::CheckoutBuilder, BranchType, IndexAddOption, Repository, Status, StatusOptions};
use serde::Serialize;

use crate::cli::GitCli;
use crate::recovery::{RecoveryKind, SnapshotPolicy};

// ---------------------------------------------------------------------------
// Shared status badge + branch info (pure)
// ---------------------------------------------------------------------------

/// A single overlay badge for a file or (rolled-up) folder.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Debug)]
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

/// Full staged/unstaged change list for the repo enclosing `dir`. Returns an
/// empty list (not an error) when `dir` isn't inside a repo or the repo is bare.
pub fn changes(dir: &str) -> GitChanges {
    let repo = match Repository::discover(dir) {
        Ok(r) => r,
        Err(_) => return GitChanges::default(),
    };
    let Some(wd) = repo.workdir().map(|p| p.to_path_buf()) else {
        return GitChanges::default();
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
                .trim_end_matches(['/', '\\'])
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
    GitChanges {
        repo_root: Some(
            wd.to_string_lossy()
                .trim_end_matches(['/', '\\'])
                .to_string(),
        ),
        branch,
        staged,
        unstaged,
    }
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
pub fn branches(path: &str) -> Result<Vec<FsBranch>, String> {
    let repo = Repository::discover(path)
        .map_err(|_| "not inside a git repository".to_string())?;
    let mut out = Vec::new();
    for b in repo.branches(Some(BranchType::Local)).map_err(|e| format!("Git error: {e}"))? {
        let (branch, _) = b.map_err(|e| format!("Git error: {e}"))?;
        let is_head = branch.is_head();
        if let Some(name) = branch.name().map_err(|e| format!("Git error: {e}"))?.map(String::from) {
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
pub fn remote_url(path: &str) -> Result<Option<String>, String> {
    let Ok(repo) = Repository::discover(path) else { return Ok(None) };
    let remotes = repo.remotes().map_err(|e| format!("Git error: {e}"))?;
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

/// Working-directory root of the repo enclosing `path` — the git2 equivalent of
/// `git rev-parse --show-toplevel`, as an absolute path with no trailing
/// separator. `None` when `path` isn't inside a repo or the repo is bare (no
/// working tree). `Repository::discover` walks up from any subpath, so the caller
/// can pass any entry inside the repo. Used by the File Explorer's "Open in Arbor"
/// to resolve the workdir a clicked path belongs to, in-process via libgit2.
pub fn repo_root(path: &str) -> Option<String> {
    let repo = Repository::discover(path).ok()?;
    let wd = repo.workdir()?;
    Some(wd.to_string_lossy().trim_end_matches(['/', '\\']).to_string())
}

// ---------------------------------------------------------------------------
// Status overlays (cached per repo-root) + inline light actions
// ---------------------------------------------------------------------------
//
// The explorer renders overlay badges itself, so it isn't bounded by the OS
// ~15-overlay-slot limit a shell extension hits. Results are cached per
// repo-root and reused while navigating within the same repo; the mutating
// actions below bust the cache so the next status recomputes.

/// Canonical key for path matching: backslashes → slashes, trailing slashes
/// stripped, lowercased. Mirrors `normPath()` in `FileExplorerModal.svelte`, so
/// backend badge keys line up with `normPath(entry.path)` on the FE. Lowercasing
/// is unconditional (the explorer already assumes case-insensitive matching).
fn norm_key(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
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
    /// Map: normalized child path → overlay badge. Keys match the FE's
    /// `normPath()` so a row looks up its badge with `badges[normPath(entry.path)]`.
    /// Only non-clean entries are present; everything else renders with no overlay.
    pub badges: HashMap<String, GitBadge>,
    /// Map: normalized child folder path → marker, for immediate children of
    /// `dir` that are themselves git repo roots. Populated regardless of
    /// `in_repo`, so the explorer can flag sibling/nested projects.
    pub repos: HashMap<String, RepoMarker>,
}

// — Per-repo status cache —

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

/// Slice the repo-wide status down to one directory's children: an entry that
/// *is* a direct child of `dir` (a file, or a folder git itself flagged — e.g.
/// an ignored/untracked directory reported whole) carries its own badge; a state
/// nested deeper rolls up to the immediate child folder with the strongest
/// descendant state.
///
/// Ignored is special-cased on roll-up: a folder is dimmed as "ignored" only
/// when git flagged that folder itself (a direct child entry). A folder that
/// merely *contains* an ignored file (e.g. a tracked `src/` holding one
/// `src/tmp.log`) is a normal tracked folder and must NOT be dimmed — mirrors
/// `git status --ignored`, which lists the ignored file, never its tracked
/// parent. Without this, any tracked folder with a stray build artifact inside
/// would render greyed-out as though the whole folder were ignored.
fn slice_for_dir(c: &RepoStatusCache, dir_key: &str) -> HashMap<String, GitBadge> {
    let prefix = format!("{dir_key}/");
    let mut out: HashMap<String, GitBadge> = HashMap::new();
    for (path_key, badge) in &c.files {
        let Some(rest) = path_key.strip_prefix(&prefix) else { continue };
        let Some(child_seg) = rest.split('/').next().filter(|s| !s.is_empty()) else { continue };
        // `rest == child_seg` ⇒ the entry is the direct child itself; anything
        // longer is a deeper descendant rolled up onto `child_seg`.
        let is_direct = rest.len() == child_seg.len();
        // Don't let a nested Ignored descendant paint its (tracked) parent folder.
        if !is_direct && *badge == GitBadge::Ignored {
            continue;
        }
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

/// Overlay-badge status for `dir`'s entries (+ child-repo markers). Cached per
/// repo-root; pass `refresh = true` (off the fs watcher) to recompute.
pub fn status(dir: &str, refresh: bool) -> Result<FsGitStatus, String> {
    // Always flag child repos, even when `dir` itself isn't in a repo — that
    // is exactly the "folder of projects" case the markers exist for.
    let repos = scan_child_repos(dir);

    let repo = match Repository::discover(dir) {
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
        .map_err(|_| "status cache poisoned".to_string())?;
    if refresh || !guard.contains_key(&root_key) {
        guard.insert(root_key.clone(), compute_repo_status(&repo));
    }
    let c = guard.get(&root_key).expect("just inserted");
    let dir_key = norm_key(dir);
    Ok(FsGitStatus {
        in_repo: true,
        repo_root: Some(root.to_string_lossy().trim_end_matches(['/', '\\']).to_string()),
        branch: c.branch.clone(),
        detached: c.detached,
        ahead: c.ahead,
        behind: c.behind,
        badges: slice_for_dir(c, &dir_key),
        repos,
    })
}

/// Open the repo enclosing the first path and compute each path relative to its
/// working directory. Returns `(repo, workdir, rel_paths)`. Errors if the paths
/// aren't inside a repo.
fn open_repo_and_rels(paths: &[String]) -> Result<(Repository, PathBuf, Vec<String>), String> {
    let first = paths.first().ok_or_else(|| "no paths".to_string())?;
    let repo = Repository::discover(first)
        .map_err(|_| "not inside a git repository".to_string())?;
    let wd = repo
        .workdir()
        .ok_or_else(|| "bare repository".to_string())?
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
/// action) so the next [`status`] recomputes even without `refresh`.
fn invalidate_cache(repo: &Repository) {
    if let Some(wd) = repo.workdir() {
        let key = norm_key(&wd.to_string_lossy());
        if let Ok(mut guard) = cache().lock() {
            guard.remove(&key);
        }
    }
}

/// Stage paths (files / folders / deletions) in their enclosing repo.
pub fn stage(paths: &[String]) -> Result<(), String> {
    let (repo, _wd, rels) = open_repo_and_rels(paths)?;
    if rels.is_empty() {
        return Ok(());
    }
    let mut index = repo.index().map_err(|e| format!("Git error: {e}"))?;
    // add_all with per-entry pathspecs handles files, whole folders, and
    // deletions (it updates the index to match the worktree) in one pass.
    index.add_all(rels.iter().map(|s| s.as_str()), IndexAddOption::DEFAULT, None)
        .map_err(|e| format!("Git error: {e}"))?;
    index.write().map_err(|e| format!("Git error: {e}"))?;
    invalidate_cache(&repo);
    Ok(())
}

/// Unstage paths (reset to HEAD; pre-initial-commit drops them from the index).
pub fn unstage(paths: &[String]) -> Result<(), String> {
    let (repo, _wd, rels) = open_repo_and_rels(paths)?;
    if rels.is_empty() {
        return Ok(());
    }
    // revparse_single avoids the peel_to_commit libgit2 bug (see unstage_file).
    match repo.revparse_single("HEAD") {
        Ok(head) => {
            repo.reset_default(Some(&head), rels.iter().map(|s| s.as_str()))
                .map_err(|e| format!("unstage: {e}"))?;
        }
        Err(_) => {
            // Pre-initial-commit: nothing committed yet → drop from the index.
            let mut index = repo.index().map_err(|e| format!("Git error: {e}"))?;
            for rel in &rels {
                let _ = index.remove_path(Path::new(rel));
            }
            index.write().map_err(|e| format!("Git error: {e}"))?;
        }
    }
    invalidate_cache(&repo);
    Ok(())
}

/// Discard working-tree changes for `paths`. When `snapshot` is `Some`, takes a
/// recovery snapshot (the Recovery-tab safety net) BEFORE discarding, using the
/// caller-injected git invoker + retention policy. Untracked entries are removed
/// from disk; tracked ones are checked out from the index (force).
pub fn discard(
    paths: &[String],
    snapshot: Option<(&GitCli, &SnapshotPolicy)>,
) -> Result<(), String> {
    let (repo, wd, rels) = open_repo_and_rels(paths)?;
    if rels.is_empty() {
        return Ok(());
    }

    // Safety net: snapshot the workdir so an over-eager discard from the
    // explorer can be undone from Arbor's Recovery tab. Best-effort — a failed
    // snapshot must never block the discard.
    if let Some((git, policy)) = snapshot {
        let _ = crate::recovery::snapshot_with_policy(
            git,
            &repo,
            RecoveryKind::Discard,
            format!("discard {} item(s) from File Explorer", rels.len()),
            policy,
        );
    }

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
        repo.checkout_index(None, Some(&mut checkout)).map_err(|e| format!("Git error: {e}"))?;
    }
    invalidate_cache(&repo);
    Ok(())
}

/// Append `paths` to the repo's `.gitignore` (anchored to the repo root; folders
/// get a trailing slash so the pattern only matches directories).
pub fn ignore(paths: &[String]) -> Result<(), String> {
    let (repo, wd, rels) = open_repo_and_rels(paths)?;
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
    std::fs::write(&gitignore, out).map_err(|e| e.to_string())?;
    invalidate_cache(&repo);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_key_slashes_trailing_and_case() {
        assert_eq!(norm_key(r"C:\Repo\Src\"), "c:/repo/src");
        assert_eq!(norm_key("/home/User/Proj//"), "/home/user/proj");
        assert_eq!(norm_key("Already/Clean"), "already/clean");
    }

    fn cache_of(files: &[(&str, GitBadge)]) -> RepoStatusCache {
        RepoStatusCache {
            files: files.iter().map(|(p, b)| ((*p).to_string(), *b)).collect(),
            branch: None,
            detached: false,
            ahead: 0,
            behind: 0,
        }
    }

    #[test]
    fn direct_child_file_keeps_its_own_badge() {
        let c = cache_of(&[("/r/foo.txt", GitBadge::Modified)]);
        let out = slice_for_dir(&c, "/r");
        assert_eq!(out.get("/r/foo.txt"), Some(&GitBadge::Modified));
    }

    #[test]
    fn ignored_directory_reported_whole_is_flagged() {
        // git reports an ignored dir as its own entry (trailing slash stripped
        // by norm_key), so it's a DIRECT child and must stay dimmed.
        let c = cache_of(&[("/r/target", GitBadge::Ignored)]);
        let out = slice_for_dir(&c, "/r");
        assert_eq!(out.get("/r/target"), Some(&GitBadge::Ignored));
    }

    #[test]
    fn nested_ignored_file_does_not_dim_tracked_parent() {
        // The #16 regression: a stray ignored artifact deep inside a tracked
        // folder must NOT paint the folder as ignored.
        let c = cache_of(&[("/r/src/build.tmp", GitBadge::Ignored)]);
        let out = slice_for_dir(&c, "/r");
        assert!(out.get("/r/src").is_none(), "tracked parent wrongly flagged ignored");
    }

    #[test]
    fn nested_non_ignored_still_rolls_up_strongest() {
        let c = cache_of(&[
            ("/r/src/a.rs", GitBadge::Modified),
            ("/r/src/b.rs", GitBadge::Untracked),
            ("/r/src/gen.tmp", GitBadge::Ignored), // ignored: skipped on roll-up
        ]);
        let out = slice_for_dir(&c, "/r");
        assert_eq!(out.get("/r/src"), Some(&GitBadge::Modified));
    }

    #[test]
    fn sibling_prefix_folders_do_not_cross_contaminate() {
        // `/r/app2/...` must not be sliced under a view of `/r/app`.
        let c = cache_of(&[("/r/app2/x.rs", GitBadge::Modified)]);
        let out = slice_for_dir(&c, "/r/app");
        assert!(out.is_empty());
    }

    #[test]
    fn conflicted_outranks_modified_on_rollup() {
        let c = cache_of(&[
            ("/r/src/a.rs", GitBadge::Modified),
            ("/r/src/deep/b.rs", GitBadge::Conflicted),
        ]);
        let out = slice_for_dir(&c, "/r");
        assert_eq!(out.get("/r/src"), Some(&GitBadge::Conflicted));
    }
}

/// Switch the repo enclosing `path` to `branch` (a local branch name). Uses a
/// SAFE checkout — refuses to overwrite uncommitted changes rather than
/// clobbering them, surfacing a clear error the explorer turns into a toast.
pub fn checkout(path: &str, branch: &str) -> Result<(), String> {
    let repo = Repository::discover(path)
        .map_err(|_| "not inside a git repository".to_string())?;
    let (object, reference) = repo
        .revparse_ext(branch)
        .map_err(|e| format!("unknown branch '{branch}': {e}"))?;
    repo.checkout_tree(&object, None).map_err(|e| {
        format!("checkout failed — commit or stash your changes first ({e})")
    })?;
    match reference {
        Some(r) => {
            let name = r
                .name()
                .ok_or_else(|| "invalid ref name".to_string())?;
            repo.set_head(name).map_err(|e| format!("Git error: {e}"))?;
        }
        None => repo.set_head_detached(object.id()).map_err(|e| format!("Git error: {e}"))?,
    }
    invalidate_cache(&repo);
    Ok(())
}
