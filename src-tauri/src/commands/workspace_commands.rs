//! Shared workspace DTOs + registry-GC helpers.
//!
//! The workspace **command handlers** all moved to the generic router
//! ([`crate::ipc::platform::workspace`]); what stays here is the surface they
//! and the migrated handlers both depend on: the serde DTOs the FE decodes
//! (kept here so the shape is single-sourced regardless of which module a
//! handler lives in) plus the registry-orphan GC helpers
//! ([`forget_repo_if_orphaned`] is also called by `repo::close_repo`).

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::workspace::{registry as registry_io, RepoRegistryEntry, WorkspaceDef, WorkspaceGroup};
use crate::AppState;

// ---------------------------------------------------------------------------
// Registry-orphan GC — "forget a repo no longer in any workspace".
// ---------------------------------------------------------------------------

/// Normalise a path for recent-repos comparison: forward slashes, no trailing
/// slash, lower-cased on Windows so `C:\Foo` and `c:/foo/` collapse to one key.
fn norm_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// Drop a path from the recent-repos list (best-effort, no-op when absent or
/// when the repo has no on-disk path). Centralised so every "forget a repo"
/// path cleans the same surface.
///
/// `pub(crate)` so the migrated `delete_registry_repo` handler reuses it.
pub(crate) fn forget_recent_repo(state: &AppState, path: &str) -> Result<()> {
    if path.trim().is_empty() { return Ok(()); }
    let mut cfg = state.lock_config()?;
    let target = norm_path(path);
    let before = cfg.recent_repos.len();
    cfg.recent_repos.retain(|p| norm_path(p) != target);
    if cfg.recent_repos.len() != before {
        let _ = crate::config::app_config::save(&cfg);
    }
    Ok(())
}

/// "Forget" a repo once it's no longer a member of any workspace.
///
/// When the user removes a repo from its last workspace — or deletes the
/// workspace that held it — Arbor drops the registry entry and its recent-repos
/// pointer, so a later import no longer matches it as "use existing". The folder
/// on disk is never touched: this is purely about Arbor forgetting it.
///
/// Guards:
/// - still referenced by another workspace → not an orphan, left alone.
/// - currently open in a tab → kept (a tab whose repo vanished from the registry
///   would break); it'll be cleaned up the normal way when the tab is closed.
///
/// Fires `on_repo_deregistered` (so plugins drop per-repo caches) and returns
/// `true` when the entry was actually removed. The caller must hold no locks.
///
/// `pub(crate)` so `repo::close_repo` can run the same GC when an orphan's last
/// tab closes — keeping "forget an orphan" in one place.
pub(crate) fn forget_repo_if_orphaned(
    state: &AppState,
    repo_id: &str,
    reason: &str,
) -> Result<bool> {
    // Still a member somewhere? Then it's not an orphan.
    if state.lock_workspaces()?.repo_is_in_any_workspace(repo_id) {
        return Ok(false);
    }
    // Need path + name for the recent-repos cleanup and the hook payload.
    let entry = {
        let reg = state.lock_repo_registry()?;
        reg.get(repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
    };
    let Some((path, name)) = entry else { return Ok(false); };
    // Don't yank a repo out from under an open tab.
    let in_open_tab = state.lock_repos()
        .map(|mgr| mgr.all_info().iter().any(|i| i.path == path))
        .unwrap_or(false);
    if in_open_tab { return Ok(false); }
    // Drop the registry entry.
    {
        let mut reg = state.lock_repo_registry()?;
        reg.remove(repo_id);
        registry_io::save(&reg)?;
    }
    // Drop the recent-repos pointer too.
    let _ = forget_recent_repo(state, &path);
    state.fire_hook("on_repo_deregistered", serde_json::json!({
        "repo_id": repo_id,
        "path":    path,
        "name":    name,
        "reason":  reason,
    }));
    Ok(true)
}

// ---------------------------------------------------------------------------
// Aggregate DTO — single round-trip to hydrate the workspace dropdown.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WorkspacesSnapshot {
    pub workspaces:          Vec<WorkspaceDef>,
    pub groups:              Vec<WorkspaceGroup>,
    pub active_workspace_id: Option<String>,
}

/// Registry entry augmented with the canonical path of its `.git` common
/// directory.  All worktrees of the same repository share that value, which
/// the UI uses to group linked worktrees together (so a secondary worktree
/// shows up next to its main repo even when it's not in any workspace).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RepoRegistryEntryWithRoot {
    pub id:           String,
    pub path:         String,
    pub remote_url:   Option<String>,
    pub display_name: String,
    /// Canonical absolute path of the `.git` common dir, or None when the
    /// path no longer points at a valid git repository (broken / moved).
    pub common_dir:   Option<String>,
    /// Current branch name if the repo's HEAD is on a branch.  None for
    /// detached HEAD or broken repos.
    pub current_branch: Option<String>,
    /// True when this path is a linked worktree (lives under
    /// `<main>/.git/worktrees/<name>`).  Lets pickers offer only "root" repos
    /// and let the user navigate to specific worktrees via the in-tab
    /// switcher instead of cluttering workspace pickers with them.
    pub is_worktree:  bool,
}

// ---------------------------------------------------------------------------
// Shared mutation/patch DTOs (the handlers that consume them live in
// `crate::ipc::platform::workspace`).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WorkspacePatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub group_id:  Option<Option<String>>, // double-option lets null clear the group
    pub repo_ids:  Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceGroupPatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub collapsed: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct RepoRegistrationResult {
    pub id:           String,
    pub existed:      bool,
    pub added_to_ws:  bool,
}

// ---------------------------------------------------------------------------
// Import / export DTOs — portable JSON so workspaces travel between machines.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedRepo {
    pub name:       String,
    pub remote_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedWorkspace {
    pub arbor_workspace_version: u32,
    pub name:                    String,
    pub color_idx:               u8,
    pub repos:                   Vec<ExportedRepo>,
}

/// Parse an imported payload and preview each repo's local status:
///   - `existing_path`: we know this repo already (matched on remote URL or
///     display name + URL) and it's on disk.
///   - `suggested_clone_dir`: nothing matched; the UI will prompt the user
///     to pick a target directory.
#[derive(Debug, Serialize)]
pub struct ImportPreviewRepo {
    pub name:          String,
    pub remote_url:    Option<String>,
    pub existing_id:   Option<String>,
    pub existing_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub name:      String,
    pub color_idx: u8,
    /// Id of an existing top-level workspace with the same (case-insensitive)
    /// name — lets the UI offer "merge" (union repos) instead of creating a
    /// duplicate.
    pub existing_workspace_id: Option<String>,
    pub repos:     Vec<ImportPreviewRepo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedGroupMember {
    pub name:      String,
    pub color_idx: u8,
    pub repos:     Vec<ExportedRepo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedWorkspaceGroup {
    pub arbor_workspace_group_version: u32,
    pub name:       String,
    pub color_idx:  u8,
    pub workspaces: Vec<ExportedGroupMember>,
}

#[derive(Debug, Serialize)]
pub struct ImportGroupPreviewWorkspace {
    pub name:         String,
    pub color_idx:    u8,
    /// Indices into `ImportGroupPreview.repos` — the (deduped) repos this
    /// member workspace contains, in declaration order.
    pub repo_indices: Vec<usize>,
    /// Id of a same-named workspace already inside the target group (only set
    /// when merging into an existing group) — the UI flags it and the commit
    /// unions repos into it instead of creating a duplicate.
    pub existing_workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportGroupPreview {
    pub name:      String,
    pub color_idx: u8,
    /// Id of an existing group with the same (case-insensitive) name, when one
    /// exists — lets the UI offer "merge into existing group" instead of
    /// silently creating a duplicate.
    pub existing_group_id: Option<String>,
    /// Deduped union of every member's repos.  Resolve each exactly once.
    pub repos:      Vec<ImportPreviewRepo>,
    pub workspaces: Vec<ImportGroupPreviewWorkspace>,
}

#[derive(Debug, Deserialize)]
pub struct ImportGroupWorkspaceCommit {
    pub name:      String,
    pub color_idx: u8,
    pub repo_ids:  Vec<String>,
    /// Existing workspace id to union repos into (idempotent merge). Honoured
    /// only when the group itself is being merged into.
    #[serde(default)]
    pub merge_into: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedBundle {
    pub arbor_workspace_bundle_version: u32,
    pub groups:     Vec<ExportedWorkspaceGroup>,
    pub workspaces: Vec<ExportedWorkspace>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportBundleResult {
    pub groups_created:     usize,
    pub groups_merged:      usize,
    pub workspaces_created: usize,
    pub workspaces_merged:  usize,
    pub repos_linked:       usize,
    pub repos_pending:      usize,
}

// ---------------------------------------------------------------------------
// Health scan — lightweight per-repo status (branch + ahead/behind + dirty).
// `probe_one` stays here as `pub(crate)` so the migrated `workspace_health_scan`
// handler calls it.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RepoHealth {
    pub repo_id:      String,
    pub path:         String,
    pub missing:      bool,
    pub branch:       Option<String>,
    pub ahead:        u32,
    pub behind:       u32,
    /// True when the current branch has an upstream tracking ref — lets the
    /// UI distinguish "0 ahead / 0 behind because in sync" from "0 ahead /
    /// 0 behind because no upstream is configured" and render accordingly.
    pub has_upstream: bool,
    pub dirty:        bool,
    /// True when an actual merge-like operation is in progress (MERGE_HEAD,
    /// CHERRY_PICK_HEAD, REBASE_HEAD, REVERT_HEAD) or an index entry carries
    /// the CONFLICTED bit.  Drives the red warning triangle.  Does NOT fire
    /// on a plain detached HEAD — that has its own field.
    pub conflicted:   bool,
    /// HEAD is not pointing at a local branch (checked out tag/commit, or
    /// any other "not on a branch" state).  Pull cannot proceed on a
    /// detached HEAD, so the UI shows a distinct icon + message.
    pub detached:     bool,
    /// True when this repo path is a linked worktree (not the main worktree).
    /// libgit2 `Repository::is_worktree()` returns true for a checkout living
    /// under `.git/worktrees/<name>`.  The UI shows a small worktree icon.
    pub is_worktree:  bool,
    pub error:        Option<String>,
}

pub(crate) fn probe_one(entry: &RepoRegistryEntry) -> RepoHealth {
    let mut out = RepoHealth {
        repo_id:      entry.id.clone(),
        path:         entry.path.clone(),
        missing:      false,
        branch:       None,
        ahead:        0,
        behind:       0,
        has_upstream: false,
        dirty:        false,
        conflicted:   false,
        detached:     false,
        is_worktree:  false,
        error:        None,
    };

    if !std::path::Path::new(&entry.path).exists() {
        out.missing = true;
        return out;
    }

    let repo = match git2::Repository::open(&entry.path) {
        Ok(r) => r,
        Err(e) => { out.error = Some(e.to_string()); return out; }
    };

    out.is_worktree = repo.is_worktree();

    // Branch
    if let Ok(head) = repo.head() {
        if let Some(name) = head.shorthand() { out.branch = Some(name.to_string()); }
        if head.is_branch() {
            // Ahead/behind vs upstream.  Mirrors `git::status::compute_ahead_behind`
            // so the workspace modal and the main tab agree on the numbers:
            //   1. Prefer the branch's configured upstream (branch.<n>.merge).
            //   2. Fall back to `refs/remotes/origin/<branch>` — covers repos
            //      that have `origin/<name>` locally (from a fetch) but never
            //      had tracking config set explicitly.
            if let Some(short) = head.shorthand() {
                let local_oid = head.target();
                let configured_upstream = repo
                    .find_branch(short, git2::BranchType::Local)
                    .ok()
                    .and_then(|b| b.upstream().ok())
                    .and_then(|u| u.get().target());
                let upstream_oid = configured_upstream.or_else(|| {
                    repo.refname_to_id(&format!("refs/remotes/origin/{short}")).ok()
                });
                if let (Some(l), Some(r)) = (local_oid, upstream_oid) {
                    out.has_upstream = true;
                    if let Ok((ahead, behind)) = repo.graph_ahead_behind(l, r) {
                        out.ahead  = ahead  as u32;
                        out.behind = behind as u32;
                    }
                }
            }
        } else {
            // HEAD resolves but does not point at a branch ref → detached.
            out.detached = true;
        }
    }

    // Dirty — any file not in a clean state.
    // Conflicted — an actual merge-like operation is stopped mid-way.  We
    // only trust the narrow "<OP>_HEAD" sentinel files here: the broader
    // `rebase-merge/` and `rebase-apply/` directories can linger as ambient
    // state on worktree checkouts without an actual conflict, and would
    // trigger false positives.  Unmerged index entries (`CONFLICTED` bit)
    // are an authoritative signal in all cases.
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .include_ignored(false)
        .exclude_submodules(true);
    if let Ok(statuses) = repo.statuses(Some(&mut opts)) {
        out.dirty = statuses.iter().any(|s| s.status() != git2::Status::CURRENT);
        out.conflicted = statuses.iter().any(|s| s.status().contains(git2::Status::CONFLICTED));
    }
    if !out.conflicted {
        let gitdir = repo.path();
        out.conflicted = gitdir.join("MERGE_HEAD").exists()
            || gitdir.join("CHERRY_PICK_HEAD").exists()
            || gitdir.join("REVERT_HEAD").exists()
            || gitdir.join("REBASE_HEAD").exists();
    }

    out
}

// ---------------------------------------------------------------------------
// Background-runner result DTO — returned by `workspace_fetch_all` /
// `workspace_pull_all` / `workspace_tag_all` (handlers in
// `crate::ipc::platform::workspace`).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WorkspaceFetchStartResult {
    pub job_id:     String,
    pub total:      usize,
}
