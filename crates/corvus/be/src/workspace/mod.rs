//! `workspace` domain — repo registry + workspace store + tab snapshots, owned
//! **out-of-process** by corvus-be (ADR-1: each backend owns its own
//! `repo_registry` + `workspaces`).
//!
//! The persistent state ([`registry`], [`store`], [`snapshot`]) is **file-backed,
//! reload-on-access**: corvus-be is the authority, but the shell keeps a
//! reload-on-access copy for the consumers that stay shell-side for now
//! (deep-link router, missing-repo flow, the `arbor.workspace` ns_shell
//! namespace) and writes the same files from the other process. corvus-be can't
//! compute the profile-aware paths itself, so the shell pushes them via the
//! `repo_registry_path` / `workspaces_path` / `workspace_state_dir` config
//! sections (the `recent_repos` trim is a shell-only `AppConfig` write, reached
//! via the `__forget_recent_repo` host method).
//!
//! Handlers live in the sibling modules (`workspace_query`, `workspace_mutation`,
//! `workspace_runs`); this module carries the shared DTOs + helpers they all use.
//! Fire-and-forget workspace hooks fire **inline** here (the host is co-located —
//! Wave 0), exactly as the shell's migrated handlers fired them through the
//! platform `post_hooks` table.

pub mod registry;
pub mod snapshot;
pub mod store;

pub use registry::RepoRegistryEntry;
pub use snapshot::{CrossWsTabRef, TabMeta, TabSnapshot};
pub use store::{WorkspaceDef, WorkspaceGroup, SCRATCH_ID};

use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Shared DTOs — wire shapes the FE decodes (serde-identical to the shell's
// `crate::commands::workspace_commands`).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WorkspacesSnapshot {
    pub workspaces:          Vec<WorkspaceDef>,
    pub groups:              Vec<WorkspaceGroup>,
    pub active_workspace_id: Option<String>,
}

/// Registry entry augmented with the canonical `.git` common-dir path (shared by
/// all worktrees of a repo) + current branch + worktree flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRegistryEntryWithRoot {
    pub id:             String,
    pub path:           String,
    pub remote_url:     Option<String>,
    pub display_name:   String,
    pub common_dir:     Option<String>,
    pub current_branch: Option<String>,
    pub is_worktree:    bool,
}

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
    pub id:          String,
    pub existed:     bool,
    pub added_to_ws: bool,
}

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

#[derive(Debug, Serialize)]
pub struct ImportPreviewRepo {
    pub name:          String,
    pub remote_url:    Option<String>,
    pub existing_id:   Option<String>,
    pub existing_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub name:                  String,
    pub color_idx:             u8,
    pub existing_workspace_id: Option<String>,
    pub repos:                 Vec<ImportPreviewRepo>,
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
    pub name:                  String,
    pub color_idx:             u8,
    pub repo_indices:          Vec<usize>,
    pub existing_workspace_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportGroupPreview {
    pub name:              String,
    pub color_idx:         u8,
    pub existing_group_id: Option<String>,
    pub repos:             Vec<ImportPreviewRepo>,
    pub workspaces:        Vec<ImportGroupPreviewWorkspace>,
}

#[derive(Debug, Deserialize)]
pub struct ImportGroupWorkspaceCommit {
    pub name:      String,
    pub color_idx: u8,
    pub repo_ids:  Vec<String>,
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

#[derive(Debug, Clone, Serialize)]
pub struct RepoHealth {
    pub repo_id:      String,
    pub path:         String,
    pub missing:      bool,
    pub branch:       Option<String>,
    pub ahead:        u32,
    pub behind:       u32,
    pub has_upstream: bool,
    pub dirty:        bool,
    pub conflicted:   bool,
    pub detached:     bool,
    pub is_worktree:  bool,
    pub error:        Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceFetchStartResult {
    pub job_id: String,
    pub total:  usize,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Broadcast that the registry / workspace membership changed, so every window
/// reloads its Projects view. Carries no payload — listeners re-query.
pub(crate) fn emit_registry_changed(state: &CorvusState) {
    state.emit("arbor://registry-changed", Value::Null);
}

/// The `workspace_payload` shape the workspace hooks carry — mirrors the shell's
/// `crate::ipc::platform::workspace::workspace_payload`.
pub(crate) fn workspace_payload(ws: &WorkspaceDef) -> Value {
    json!({
        "id":         ws.id,
        "name":       ws.name,
        "color_idx":  ws.color_idx,
        "repo_ids":   ws.repo_ids,
        "group_id":   ws.group_id,
        "repo_count": ws.repo_ids.len(),
    })
}

/// Augment a registry entry with its `.git` common dir + current branch +
/// worktree flag (opens the repo with libgit2; broken/missing → all `None`).
pub(crate) fn entry_with_root(e: RepoRegistryEntry) -> RepoRegistryEntryWithRoot {
    let mut common_dir: Option<String> = None;
    let mut current_branch: Option<String> = None;
    let mut is_worktree = false;
    if let Ok(repo) = git2::Repository::open(&e.path) {
        common_dir = std::fs::canonicalize(repo.commondir()).ok().map(|p| {
            let s = p.to_string_lossy().to_string();
            let s = s.strip_prefix(r"\\?\").map(|x| x.to_string()).unwrap_or(s);
            s.replace('\\', "/").trim_end_matches('/').to_string()
        });
        current_branch = repo.head().ok().and_then(|h| h.shorthand().map(|s| s.to_string()));
        is_worktree = repo.is_worktree();
    }
    RepoRegistryEntryWithRoot {
        id: e.id,
        path: e.path,
        remote_url: e.remote_url,
        display_name: e.display_name,
        common_dir,
        current_branch,
        is_worktree,
    }
}

/// Lightweight per-repo status (branch + ahead/behind + dirty + conflicted).
/// Verbatim port of the shell's `probe_one` — no `AppState`, pure libgit2.
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

    if let Ok(head) = repo.head() {
        if let Some(name) = head.shorthand() { out.branch = Some(name.to_string()); }
        if head.is_branch() {
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
            out.detached = true;
        }
    }

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

/// Drop a path from the shell's recent-repos list (a shell-only `AppConfig`
/// write) over the reverse channel. Best-effort, mirrors the shell's
/// `forget_recent_repo`.
pub(crate) fn forget_recent_repo(state: &CorvusState, path: &str) {
    if path.trim().is_empty() { return; }
    let _ = state.host_call("__forget_recent_repo", json!({ "path": path }));
}

/// "Forget" a repo once it's no longer a member of any workspace: drop the
/// registry entry + its recent-repos pointer, and fire `on_repo_deregistered`.
/// Verbatim port of the shell's `forget_repo_if_orphaned`, with the open-tab
/// guard reading the shell-pushed open set ([`CorvusState::open_tabs`]) instead
/// of the shell's `RepoManager`. Returns `true` when the entry was removed.
pub(crate) fn forget_repo_if_orphaned(
    state: &CorvusState,
    repo_id: &str,
    reason: &str,
) -> Result<bool, String> {
    // Still a member somewhere? Then it's not an orphan.
    if store::store(state).repo_is_in_any_workspace(repo_id) {
        return Ok(false);
    }
    // Need path + name for the recent-repos cleanup and the hook payload.
    let entry = {
        let reg = registry::registry(state);
        reg.get(repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
    };
    let Some((path, name)) = entry else { return Ok(false); };
    // Don't yank a repo out from under an open tab.
    let in_open_tab = state.open_tabs().iter().any(|(_, p)| p == &path);
    if in_open_tab { return Ok(false); }
    // Drop the registry entry.
    registry::mutate(state, |reg| { reg.remove(repo_id); Ok(()) })?;
    // Drop the recent-repos pointer too.
    forget_recent_repo(state, &path);
    state.fire_hook("on_repo_deregistered", json!({
        "repo_id": repo_id,
        "path":    path,
        "name":    name,
        "reason":  reason,
    }));
    Ok(true)
}
