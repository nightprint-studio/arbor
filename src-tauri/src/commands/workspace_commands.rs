use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;
use crate::error::{AppError, Result};
use crate::jobs::{JobInfo, JobStatus, JobRegistry};
use crate::workspace::{
    registry as registry_io, snapshot as snapshot_io, store as store_io,
    RepoRegistryEntry, WorkspaceDef, WorkspaceGroup,
    SCRATCH_ID,
};

// ---------------------------------------------------------------------------
// Hook helpers — workspace events reach plugins via the regular hook pipe.
// Each payload is kept minimal: plugins that need more should query back via
// arbor.workspace.* APIs (Phase 7) so we avoid schema drift here.
// ---------------------------------------------------------------------------

fn fire_hook(app: &AppHandle, hook: &str, payload: serde_json::Value) {
    app.state::<AppState>().fire_hook(hook, payload);
}

/// Broadcast that the repo registry and/or workspace membership changed, so
/// every window (the main app AND any standalone File Explorer window) can
/// reload its Projects view. Carries no payload — listeners re-query.
fn emit_registry_changed(app: &AppHandle) {
    let _ = app.emit("arbor://registry-changed", ());
}

fn workspace_payload(ws: &WorkspaceDef) -> serde_json::Value {
    serde_json::json!({
        "id":        ws.id,
        "name":      ws.name,
        "color_idx": ws.color_idx,
        "repo_ids":  ws.repo_ids,
        "group_id":  ws.group_id,
        "repo_count": ws.repo_ids.len(),
    })
}

/// Normalise a path for recent-repos comparison (mirror of the helper in
/// `missing_commands`): forward slashes, no trailing slash, lower-cased on
/// Windows so `C:\Foo` and `c:/foo/` collapse to the same key.
fn norm_path(p: &str) -> String {
    let s = p.replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) { s.to_lowercase() } else { s }
}

/// Drop a path from the recent-repos list (best-effort, no-op when absent or
/// when the repo has no on-disk path). Centralised so every "forget a repo"
/// path cleans the same surface.
fn forget_recent_repo(state: &AppState, path: &str) -> Result<()> {
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
/// `pub(crate)` so `repo_commands::close_repo` can run the same GC when an
/// orphan's last tab closes — keeping "forget an orphan" in one place.
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

// ---------------------------------------------------------------------------
// Query commands — `list_workspaces`, `list_registry_repos`,
// `list_registry_with_roots`, `load_workspace_snapshot` migrated to
// `crate::ipc::platform::workspace`.
// ---------------------------------------------------------------------------

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
// Mutation commands — workspaces (all DEFERRED: emit `arbor://registry-changed`
// / `arbor://workspace-switched` FE events; left inline for the emit/seam pass).
// ---------------------------------------------------------------------------

// `create_workspace` migrated to `crate::ipc::platform::workspace`
// (`on_workspace_created` fires from platform `post_hooks`).

// `WorkspacePatch` stays here (shared DTO imported by the migrated
// `update_workspace` handler).
#[derive(Debug, Deserialize)]
pub struct WorkspacePatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub group_id:  Option<Option<String>>, // double-option lets null clear the group
    pub repo_ids:  Option<Vec<String>>,
}

// `update_workspace` migrated to `crate::ipc::platform::workspace`
// (`on_workspace_updated` fires from platform `post_hooks`).

#[tauri::command]
pub fn delete_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<()> {
    if workspace_id == SCRATCH_ID {
        return Err(AppError::Other("cannot delete the Scratch workspace".into()));
    }
    // Capture the payload and the member list before mutating: the members are
    // the GC candidates once the workspace is gone.
    let (deleted_payload, member_ids) = {
        let store = state.lock_workspaces()?;
        let payload = store.get(&workspace_id).map(workspace_payload);
        let members = store.get(&workspace_id).map(|w| w.repo_ids.clone()).unwrap_or_default();
        (payload, members)
    };
    {
        let mut store = state.lock_workspaces()?;
        store.remove(&workspace_id)?;
        store_io::save(&store)?;
    }
    // Best-effort: delete the snapshot file too.
    let _ = snapshot_io::delete(&workspace_id);
    if let Some(payload) = deleted_payload {
        fire_hook(&app, "on_workspace_deleted", payload);
    }
    // Forget every member that's no longer referenced by another workspace, so
    // Arbor stops proposing it as "use existing" on a later import.
    for repo_id in member_ids {
        let _ = forget_repo_if_orphaned(&state, &repo_id, "workspace_deleted");
    }
    emit_registry_changed(&app);
    Ok(())
}

// `reorder_workspaces` migrated to `crate::ipc::platform::workspace`
// (no hook; emits `arbor://registry-changed` via the event sink).

#[tauri::command]
pub fn set_active_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<WorkspaceDef> {
    let (from_id, ws) = {
        let mut store = state.lock_workspaces()?;
        let target = store.get(&workspace_id).cloned()
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        let from = store.active_workspace_id.clone();
        store.active_workspace_id = Some(workspace_id.clone());
        store_io::save(&store)?;
        (from, target)
    };
    let mut payload = workspace_payload(&ws);
    if let Some(from) = from_id {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("from_id".into(), serde_json::Value::String(from));
        }
    }
    let _ = app.emit("arbor://workspace-switched", &payload);
    fire_hook(&app, "on_workspace_switched", payload);
    Ok(ws)
}

// ---------------------------------------------------------------------------
// Mutation commands — groups. The handlers migrated to
// `crate::ipc::platform::workspace`; `WorkspaceGroupPatch` stays here (shared
// DTO imported by the migrated handler).
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct WorkspaceGroupPatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub collapsed: Option<bool>,
}

// ---------------------------------------------------------------------------
// Mutation commands — repo membership (all DEFERRED: fire hooks + emit
// `arbor://registry-changed`; left inline for the emit/seam pass).
// ---------------------------------------------------------------------------

// `add_repo_to_workspace` migrated to `crate::ipc::platform::workspace`
// (`on_workspace_repo_added` fires from platform `post_hooks`).

#[tauri::command]
pub fn remove_repo_from_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    repo_id: String,
) -> Result<()> {
    {
        let mut store = state.lock_workspaces()?;
        store.remove_repo(&workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    fire_hook(&app, "on_workspace_repo_removed", serde_json::json!({
        "workspace_id": workspace_id,
        "repo_id":      repo_id,
    }));

    // If that was the repo's last workspace, forget it entirely (drop the
    // registry entry + recent-repos pointer, fire on_repo_deregistered). Kept
    // when it's still open in a tab — close_repo runs the same GC on tab close.
    let _ = forget_repo_if_orphaned(&state, &repo_id, "removed_from_last_workspace");
    emit_registry_changed(&app);
    Ok(())
}

// `move_repo_between_workspaces` migrated to `crate::ipc::platform::workspace`
// (`on_workspace_repo_removed` + `on_workspace_repo_added` fire from platform
// `post_hooks`).

// ---------------------------------------------------------------------------
// Repo registry — registration + editing + removal
// ---------------------------------------------------------------------------

// `RepoRegistrationResult` stays here (shared DTO imported by the migrated
// `register_repo_path` handler).
#[derive(Debug, Serialize)]
pub struct RepoRegistrationResult {
    pub id:           String,
    pub existed:      bool,
    pub added_to_ws:  bool,
}

// `register_repo_path` / `register_pending_repo` / `update_registry_repo`
// migrated to `crate::ipc::platform::workspace` (no hooks; each emits
// `arbor://registry-changed` via the event sink).

/// Fully deregister a repo — removes it from the registry and from every
/// workspace membership.  The path on disk is NOT touched.
///
/// Fires `on_repo_deregistered` so plugins can drop per-repo caches stored
/// outside the repo (e.g. deps-explorer's tree-cache keyed by the absolute
/// module dir).
#[tauri::command]
pub fn delete_registry_repo(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<()> {
    // Capture path/name BEFORE removal so the hook payload is meaningful.
    let path_name = {
        let reg = state.lock_repo_registry()?;
        reg.get(&repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
    };
    {
        let mut store = state.lock_workspaces()?;
        store.purge_repo_everywhere(&repo_id);
        store_io::save(&store)?;
    }
    {
        let mut reg = state.lock_repo_registry()?;
        reg.remove(&repo_id);
        registry_io::save(&reg)?;
    }
    if let Some((path, name)) = path_name {
        let _ = forget_recent_repo(&state, &path);
        fire_hook(&app, "on_repo_deregistered", serde_json::json!({
            "repo_id": repo_id,
            "path":    path,
            "name":    name,
            "reason":  "registry_delete",
        }));
    }
    emit_registry_changed(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tab snapshots — persisted per workspace.  The frontend owns tab state and
// pushes the complete snapshot whenever it changes; we just write it out.
// ---------------------------------------------------------------------------

// `save_workspace_snapshot` migrated to `crate::ipc::platform::workspace`.

// ---------------------------------------------------------------------------
// Import / export — portable JSON so workspaces travel between machines.
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

// `export_workspace` migrated to `crate::ipc::platform::workspace`.

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

// `import_workspace_preview` migrated to `crate::ipc::platform::workspace`.

/// Create a workspace from a list of already-resolved repo ids — or, when
/// `merge_into` names an existing workspace, union the ids into it instead of
/// creating a duplicate.  The frontend does the per-repo Locate/Clone/Skip
/// dance and passes us the final list of registry ids to wrap up.
#[tauri::command]
pub fn import_workspace_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
    merge_into: Option<String>,
) -> Result<WorkspaceDef> {
    let (ws, merged) = {
        let mut store = state.lock_workspaces()?;
        let result = match merge_into.filter(|id| store.get(id).is_some()) {
            Some(id) => {
                store.merge_repos_into(&id, &repo_ids)?;
                let ws = store.get(&id).cloned()
                    .ok_or_else(|| AppError::Other(format!("workspace not found: {id}")))?;
                (ws, true)
            }
            None => (store.create(name, color_idx, repo_ids, group_id), false),
        };
        store_io::save(&store)?;
        result
    };
    fire_hook(
        &app,
        if merged { "on_workspace_updated" } else { "on_workspace_created" },
        workspace_payload(&ws),
    );
    Ok(ws)
}

// ---------------------------------------------------------------------------
// Import / export — workspace GROUPS.
//
// A group bundle is just the group's cosmetic data plus a self-contained
// `ExportedWorkspace`-shaped block per child workspace (each carrying its own
// repo list), so a single workspace can still be lifted out of the bundle by
// hand.  Dedup of repos shared across several member workspaces happens at
// preview time, keyed on remote URL (falling back to display name for
// local-only repos), so the user resolves each repo once.
// ---------------------------------------------------------------------------

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

// `export_workspace_group` migrated to `crate::ipc::platform::workspace`.

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

// `import_workspace_group_preview` migrated to `crate::ipc::platform::workspace`.

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

/// Create (or merge into) a group and all of its member workspaces from the
/// already-resolved repo ids the frontend produced.  When `existing_group_id`
/// names a still-present group the members are merged into it (same-named
/// workspaces have their repos unioned, the rest are added); otherwise a fresh
/// group with all-new workspaces is created.
#[tauri::command]
pub fn import_workspace_group_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    color_idx: u8,
    existing_group_id: Option<String>,
    workspaces: Vec<ImportGroupWorkspaceCommit>,
) -> Result<()> {
    // (workspace, was_merged) for each member, to fire the right hook after.
    let touched: Vec<(WorkspaceDef, bool)> = {
        let mut store = state.lock_workspaces()?;
        let (group_id, merged_group) = match existing_group_id.filter(|id| store.get_group(id).is_some()) {
            Some(id) => (id, true),
            None     => (store.create_group(name, color_idx).id, false),
        };
        let mut touched = Vec::with_capacity(workspaces.len());
        for w in workspaces {
            // Merge only when reusing the existing group AND the target still
            // exists — otherwise the id could point at an orphaned workspace.
            let target = if merged_group {
                w.merge_into.filter(|id| store.get(id).is_some())
            } else {
                None
            };
            match target {
                Some(id) => {
                    store.merge_repos_into(&id, &w.repo_ids)?;
                    if let Some(ws) = store.get(&id).cloned() { touched.push((ws, true)); }
                }
                None => {
                    let ws = store.create(w.name, w.color_idx, w.repo_ids, Some(group_id.clone()));
                    touched.push((ws, false));
                }
            }
        }
        store_io::save(&store)?;
        touched
    };
    for (ws, merged) in &touched {
        fire_hook(
            &app,
            if *merged { "on_workspace_updated" } else { "on_workspace_created" },
            workspace_payload(ws),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Full backup — every group (with members) + every top-level workspace in one
// portable bundle.  Scratch is excluded (it's an ephemeral catch-all, not a
// curated workspace worth restoring).
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportedBundle {
    pub arbor_workspace_bundle_version: u32,
    pub groups:     Vec<ExportedWorkspaceGroup>,
    pub workspaces: Vec<ExportedWorkspace>,
}

// `export_all_workspaces` migrated to `crate::ipc::platform::workspace`.

#[derive(Debug, Default, Serialize)]
pub struct ImportBundleResult {
    pub groups_created:     usize,
    pub groups_merged:      usize,
    pub workspaces_created: usize,
    pub workspaces_merged:  usize,
    pub repos_linked:       usize,
    pub repos_pending:      usize,
}

/// Restore a full bundle in one shot.  Non-blocking, like the single-workspace
/// import: every repo is resolved against the registry by remote URL; matches
/// are linked, the rest land as "pending" (not-cloned) entries to clone/locate
/// later.  Groups and workspaces are reconciled by name (idempotent merge), so
/// re-restoring the same bundle changes nothing.
#[tauri::command]
pub fn import_bundle_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ExportedBundle,
) -> Result<ImportBundleResult> {
    // Dedup key: prefer the remote URL, fall back to the display name.
    fn dedup_key(r: &ExportedRepo) -> String {
        match r.remote_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => format!("url:{}", u.to_lowercase()),
            None    => format!("name:{}", r.name.trim().to_lowercase()),
        }
    }

    let mut result = ImportBundleResult::default();

    // Pass 1 — resolve every distinct repo across the bundle to a registry id
    // (existing match, or a fresh pending entry). One registry write for all.
    let mut key_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    {
        let mut reg = state.lock_repo_registry()?;
        let mut all: Vec<&ExportedRepo> = Vec::new();
        for g in &payload.groups { for w in &g.workspaces { all.extend(w.repos.iter()); } }
        for w in &payload.workspaces { all.extend(w.repos.iter()); }
        for r in all {
            let key = dedup_key(r);
            if key_to_id.contains_key(&key) { continue; }
            // Clone the matched id up front so the immutable registry borrow
            // ends before the (mutable) insert_pending in the None arm.
            let existing = r.remote_url.as_deref().map(str::trim).filter(|u| !u.is_empty())
                .and_then(|u| reg.find_by_remote_url(u))
                .map(|e| e.id.clone());
            let id = match existing {
                Some(id) => { result.repos_linked += 1; id }
                None     => { result.repos_pending += 1; reg.insert_pending(r.remote_url.clone(), &r.name) }
            };
            key_to_id.insert(key, id);
        }
        registry_io::save(&reg)?;
    }

    let ids_of = |repos: &[ExportedRepo]| -> Vec<String> {
        let mut out = Vec::new();
        for r in repos {
            if let Some(id) = key_to_id.get(&dedup_key(r)) {
                if !out.contains(id) { out.push(id.clone()); }
            }
        }
        out
    };

    // Pass 2 — rebuild groups + workspaces (idempotent merge by name).
    let mut touched: Vec<(WorkspaceDef, bool)> = Vec::new();
    {
        let mut store = state.lock_workspaces()?;
        for g in &payload.groups {
            let existing_gid = store.groups.iter()
                .find(|x| x.name.eq_ignore_ascii_case(g.name.trim()))
                .map(|x| x.id.clone());
            let (gid, merged_group) = match existing_gid {
                Some(id) => { result.groups_merged += 1; (id, true) }
                None     => { result.groups_created += 1; (store.create_group(g.name.clone(), g.color_idx).id, false) }
            };
            for w in &g.workspaces {
                let ids = ids_of(&w.repos);
                let target = if merged_group {
                    store.find_by_name_in_group(&w.name, Some(&gid)).map(|x| x.id.clone())
                } else { None };
                match target {
                    Some(id) => {
                        store.merge_repos_into(&id, &ids)?;
                        if let Some(ws) = store.get(&id).cloned() { touched.push((ws, true)); }
                        result.workspaces_merged += 1;
                    }
                    None => {
                        let ws = store.create(w.name.clone(), w.color_idx, ids, Some(gid.clone()));
                        touched.push((ws, false));
                        result.workspaces_created += 1;
                    }
                }
            }
        }
        for w in &payload.workspaces {
            let ids = ids_of(&w.repos);
            let target = store.find_by_name_in_group(&w.name, None).map(|x| x.id.clone());
            match target {
                Some(id) => {
                    store.merge_repos_into(&id, &ids)?;
                    if let Some(ws) = store.get(&id).cloned() { touched.push((ws, true)); }
                    result.workspaces_merged += 1;
                }
                None => {
                    let ws = store.create(w.name.clone(), w.color_idx, ids, None);
                    touched.push((ws, false));
                    result.workspaces_created += 1;
                }
            }
        }
        store_io::save(&store)?;
    }
    for (ws, merged) in &touched {
        fire_hook(&app, if *merged { "on_workspace_updated" } else { "on_workspace_created" }, workspace_payload(ws));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Health scan — lightweight per-repo status (branch + ahead/behind + dirty).
//
// Runs on the calling thread (the command itself is called from the async
// executor so the UI stays responsive).  Uses libgit2 rather than the CLI so
// the cost is one pack-file open per repo; no fork/exec overhead.
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

// `workspace_health_scan` migrated to `crate::ipc::platform::workspace`
// (`probe_one` stays here as `pub(crate)` so the migrated handler can call it).

// ---------------------------------------------------------------------------
// Fetch-all — one aggregated Job for the whole workspace.
//
// Sequential on a single background thread.  Each repo's result is logged
// to the Job's output panel; per-repo progress updates are emitted as
// `arbor://workspace-fetch-progress` events so the modal can tick its
// per-row spinners in real time.  Errors do not abort the run — they're
// collected and reported in the final summary.
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct WorkspaceFetchStartResult {
    pub job_id:     String,
    pub total:      usize,
}

#[tauri::command]
pub fn workspace_fetch_all(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult> {
    // Freeze the list of (repo_id, path, display_name) under the lock.
    let targets: Vec<(String, String, String)> = {
        let store = state.lock_workspaces()?;
        let reg   = state.lock_repo_registry()?;
        let ws = store.get(&workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids.iter()
            .filter_map(|id| reg.get(id))
            .filter(|e| std::path::Path::new(&e.path).exists())
            .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
            .collect()
    };

    let total = targets.len();
    let job_name = format!("Fetch workspace ({total} repos)");
    let job_cmd  = format!("workspace-fetch-all:{workspace_id}");
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            job_name.clone(),
            plugin_name:     "arbor".into(),
            command:         job_cmd.clone(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".into()),
            non_cancellable: false,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    // Include every field the frontend reads out of the event — otherwise
    // `upsertJob` overwrites the registry row with `name = undefined` and
    // the job appears in the Jobs overlay with no label or category.
    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &job_name,
        "plugin_name": "arbor",
        "command":     &job_cmd,
        "category":    "System",
    }));

    let app_clone = app.clone();
    let ws_id     = workspace_id.clone();
    let jid       = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-fetch-{jid}"))
        .spawn(move || {
            let mut ok    = 0usize;
            let mut fail  = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}",
                                     n = idx + 1, total = total);
                log_and_emit(&app_clone, &jid, &header);
                let _ = app_clone.emit("arbor://workspace-fetch-progress", serde_json::json!({
                    "job_id":       &jid,
                    "workspace_id": &ws_id,
                    "repo_id":      repo_id,
                    "index":        idx,
                    "total":        total,
                    "phase":        "start",
                }));

                let result = fetch_one(path);

                match result {
                    Ok(summary) => {
                        ok += 1;
                        log_and_emit(&app_clone, &jid, &format!("  ok — {summary}"));
                        let _ = app_clone.emit("arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "ok",
                        }));
                    }
                    Err(e) => {
                        fail += 1;
                        log_and_emit(&app_clone, &jid, &format!("  error — {e}"));
                        let _ = app_clone.emit("arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "error",
                            "error":        e,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {fail} failed, {total} total");
            log_and_emit(&app_clone, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            let state = app_clone.state::<AppState>();
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.set_status(&jid, JobStatus::Completed { exit_code });
            }
            let _ = app_clone.emit("arbor://job-done", serde_json::json!({
                "job_id":    jid,
                "success":   fail == 0,
                "exit_code": exit_code,
                "summary":   summary,
            }));
            // Notify the frontend to refresh the graph for the active tab.
            let _ = app_clone.emit("arbor://workspace-fetch-done", serde_json::json!({
                "job_id":       jid,
                "workspace_id": ws_id,
                "ok":           ok,
                "failed":       fail,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn fetch thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

fn log_and_emit(app: &AppHandle, job_id: &str, line: &str) {
    let state = app.state::<AppState>();
    if let Ok(mut jobs) = state.jobs.lock() {
        jobs.append_output(job_id, line.to_string());
    }
    let _ = app.emit("arbor://job-output", serde_json::json!({
        "job_id": job_id,
        "text":   line,
    }));
}

fn fetch_one(path: &str) -> std::result::Result<String, String> {
    let repo = git2::Repository::open(path).map_err(|e| e.to_string())?;
    // Prefer "origin" if present; otherwise pick the first remote.
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    let remote_name = remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| "no remotes configured".to_string())?
        .to_string();
    let res = crate::git::remote::fetch(&repo, &remote_name).map_err(|e| e.to_string())?;
    Ok(format!("remote='{}' objects={} bytes={}",
               res.remote, res.received_objects, res.received_bytes))
}

// ---------------------------------------------------------------------------
// Pull-all — same orchestration as fetch-all but does a full
// `git::remote::pull` (fetch + fast-forward / merge) per repo.
//
// Events are emitted on a separate namespace so the modal can track fetch
// and pull runs independently.  Each repo result resolves to one of three
// phases: `ok`, `error`, or `conflict` (pull left MERGE_HEAD in .git/).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn workspace_pull_all(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult> {
    let targets: Vec<(String, String, String)> = {
        let store = state.lock_workspaces()?;
        let reg   = state.lock_repo_registry()?;
        let ws = store.get(&workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids.iter()
            .filter_map(|id| reg.get(id))
            .filter(|e| std::path::Path::new(&e.path).exists())
            .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
            .collect()
    };

    let total = targets.len();
    let job_name = format!("Pull workspace ({total} repos)");
    let job_cmd  = format!("workspace-pull-all:{workspace_id}");
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            job_name.clone(),
            plugin_name:     "arbor".into(),
            command:         job_cmd.clone(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".into()),
            non_cancellable: false,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &job_name,
        "plugin_name": "arbor",
        "command":     &job_cmd,
        "category":    "System",
    }));

    let app_clone = app.clone();
    let ws_id     = workspace_id.clone();
    let jid       = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-pull-{jid}"))
        .spawn(move || {
            let mut ok       = 0usize;
            let mut fail     = 0usize;
            let mut conflict = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}",
                                     n = idx + 1, total = total);
                log_and_emit(&app_clone, &jid, &header);
                let _ = app_clone.emit("arbor://workspace-pull-progress", serde_json::json!({
                    "job_id":       &jid,
                    "workspace_id": &ws_id,
                    "repo_id":      repo_id,
                    "index":        idx,
                    "total":        total,
                    "phase":        "start",
                }));

                match pull_one(path) {
                    PullOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&app_clone, &jid, &format!("  ok — {summary}"));
                        let _ = app_clone.emit("arbor://workspace-pull-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "ok",
                        }));
                    }
                    PullOutcome::Conflict(msg) => {
                        conflict += 1;
                        log_and_emit(&app_clone, &jid, &format!("  conflict — {msg}"));
                        let _ = app_clone.emit("arbor://workspace-pull-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "conflict",
                            "error":        msg,
                        }));
                    }
                    PullOutcome::Err(msg) => {
                        fail += 1;
                        log_and_emit(&app_clone, &jid, &format!("  error — {msg}"));
                        let _ = app_clone.emit("arbor://workspace-pull-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "error",
                            "error":        msg,
                        }));
                    }
                }
            }

            let summary = format!(
                "Done — {ok} ok, {conflict} conflict, {fail} failed, {total} total"
            );
            log_and_emit(&app_clone, &jid, &summary);

            let exit_code = if fail == 0 && conflict == 0 { 0 } else { 1 };
            let state = app_clone.state::<AppState>();
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.set_status(&jid, JobStatus::Completed { exit_code });
            }
            let _ = app_clone.emit("arbor://job-done", serde_json::json!({
                "job_id":    jid,
                "success":   exit_code == 0,
                "exit_code": exit_code,
                "summary":   summary,
            }));
            let _ = app_clone.emit("arbor://workspace-pull-done", serde_json::json!({
                "job_id":       jid,
                "workspace_id": ws_id,
                "ok":           ok,
                "failed":       fail,
                "conflict":     conflict,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn pull thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum PullOutcome { Ok(String), Conflict(String), Err(String) }

fn pull_one(path: &str) -> PullOutcome {
    let repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(e) => return PullOutcome::Err(e.to_string()),
    };

    // Refuse detached HEAD up front — `git::remote::pull` would surface a
    // less helpful error deep inside libgit2.  A clear message lets the UI
    // suggest checking out a branch first.
    if let Ok(head) = repo.head() {
        if !head.is_branch() {
            return PullOutcome::Err(
                "detached HEAD — check out a branch to pull".into()
            );
        }
    }

    // Already mid-operation: skip the pull and surface it as a conflict so
    // the user knows this repo needs attention before the next run.
    let gitdir = repo.path().to_path_buf();
    let has_merge = |p: &std::path::Path| p.join("MERGE_HEAD").exists()
        || p.join("REBASE_HEAD").exists()
        || p.join("CHERRY_PICK_HEAD").exists()
        || p.join("REVERT_HEAD").exists();
    if has_merge(&gitdir) {
        return PullOutcome::Conflict("repo already has an unresolved merge/rebase".into());
    }

    let remotes = match repo.remotes() {
        Ok(r) => r,
        Err(e) => return PullOutcome::Err(e.to_string()),
    };
    let remote_name = match remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
    {
        Some(n) => n.to_string(),
        None => return PullOutcome::Err("no remotes configured".into()),
    };

    match crate::git::remote::pull(&repo, &remote_name) {
        Ok(()) => PullOutcome::Ok(format!("pulled from '{remote_name}'")),
        Err(e) => {
            // A left-over MERGE_HEAD after a failed pull is the signature of
            // an in-progress merge with conflicts — surface it distinctly so
            // the UI can draw the conflict warning on that row.
            if has_merge(&gitdir) {
                PullOutcome::Conflict(e.to_string())
            } else {
                PullOutcome::Err(e.to_string())
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tag-all — create the same tag on every workspace member's HEAD.
//
// The frontend modal already showed the user any pre-flight warnings
// (detached HEAD, behind upstream, dirty workdir, missing path).  By the time
// this runs the user has accepted those caveats — we still skip detached /
// missing repos because creating a tag at HEAD on those is meaningless or
// impossible.  When `push` is true, each successful tag is pushed to the
// repo's preferred remote (origin first, then the first one configured).
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn workspace_tag_all(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    tag_name: String,
    message: Option<String>,
    push: bool,
) -> Result<WorkspaceFetchStartResult> {
    let trimmed = tag_name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Other("tag name is required".into()));
    }

    let targets: Vec<(String, String, String)> = {
        let store = state.lock_workspaces()?;
        let reg   = state.lock_repo_registry()?;
        let ws = store.get(&workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids.iter()
            .filter_map(|id| reg.get(id))
            .filter(|e| std::path::Path::new(&e.path).exists())
            .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
            .collect()
    };

    let total = targets.len();
    let job_name = if push {
        format!("Tag workspace '{trimmed}' + push ({total} repos)")
    } else {
        format!("Tag workspace '{trimmed}' ({total} repos)")
    };
    let job_cmd = format!("workspace-tag-all:{workspace_id}:{trimmed}");
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            job_name.clone(),
            plugin_name:     "arbor".into(),
            command:         job_cmd.clone(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".into()),
            non_cancellable: false,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };

    let _ = app.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &job_name,
        "plugin_name": "arbor",
        "command":     &job_cmd,
        "category":    "System",
    }));

    let app_clone = app.clone();
    let ws_id     = workspace_id.clone();
    let jid       = job_id.clone();
    let tag       = trimmed.clone();
    let msg       = message.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-tag-{jid}"))
        .spawn(move || {
            let mut ok      = 0usize;
            let mut fail    = 0usize;
            let mut skipped = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}",
                                     n = idx + 1, total = total);
                log_and_emit(&app_clone, &jid, &header);
                let _ = app_clone.emit("arbor://workspace-tag-progress", serde_json::json!({
                    "job_id":       &jid,
                    "workspace_id": &ws_id,
                    "repo_id":      repo_id,
                    "index":        idx,
                    "total":        total,
                    "phase":        "start",
                }));

                match tag_one(path, &tag, msg.as_deref(), push) {
                    TagOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&app_clone, &jid, &format!("  ok — {summary}"));
                        let _ = app_clone.emit("arbor://workspace-tag-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "ok",
                        }));
                    }
                    TagOutcome::Skipped(reason) => {
                        skipped += 1;
                        log_and_emit(&app_clone, &jid, &format!("  skipped — {reason}"));
                        let _ = app_clone.emit("arbor://workspace-tag-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "skipped",
                            "error":        reason,
                        }));
                    }
                    TagOutcome::Err(e) => {
                        fail += 1;
                        log_and_emit(&app_clone, &jid, &format!("  error — {e}"));
                        let _ = app_clone.emit("arbor://workspace-tag-progress", serde_json::json!({
                            "job_id":       &jid,
                            "workspace_id": &ws_id,
                            "repo_id":      repo_id,
                            "index":        idx,
                            "total":        total,
                            "phase":        "error",
                            "error":        e,
                        }));
                    }
                }
            }

            let summary = format!(
                "Done — {ok} ok, {skipped} skipped, {fail} failed, {total} total"
            );
            log_and_emit(&app_clone, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            let state = app_clone.state::<AppState>();
            if let Ok(mut jobs) = state.jobs.lock() {
                jobs.set_status(&jid, JobStatus::Completed { exit_code });
            }
            let _ = app_clone.emit("arbor://job-done", serde_json::json!({
                "job_id":    jid,
                "success":   fail == 0,
                "exit_code": exit_code,
                "summary":   summary,
            }));
            let _ = app_clone.emit("arbor://workspace-tag-done", serde_json::json!({
                "job_id":       jid,
                "workspace_id": ws_id,
                "tag_name":     tag,
                "ok":           ok,
                "failed":       fail,
                "skipped":      skipped,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn tag thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum TagOutcome { Ok(String), Skipped(String), Err(String) }

fn tag_one(path: &str, tag_name: &str, message: Option<&str>, push: bool) -> TagOutcome {
    let repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    // Tagging in detached HEAD is technically possible, but the workspace
    // flow targets a "release tag on each project's current branch" use case.
    // A detached HEAD almost always means the user navigated to an old commit
    // — silently tagging that would be surprising, so skip and surface it.
    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };
    if !head.is_branch() {
        return TagOutcome::Skipped("detached HEAD — no branch to tag".into());
    }
    let target_oid = match head.target() {
        Some(oid) => oid,
        None      => return TagOutcome::Err("HEAD has no target".into()),
    };
    let target = match repo.find_object(target_oid, Some(git2::ObjectType::Commit)) {
        Ok(o)  => o,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    let create_res = if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        match repo.signature() {
            Ok(sig) => repo.tag(tag_name, &target, &sig, msg, false).map(|_| "annotated"),
            Err(e)  => return TagOutcome::Err(e.to_string()),
        }
    } else {
        repo.tag_lightweight(tag_name, &target, false).map(|_| "lightweight")
    };
    let kind = match create_res {
        Ok(k) => k,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    if !push {
        return TagOutcome::Ok(format!("{kind} tag at {}", &target_oid.to_string()[..8]));
    }

    let remotes = match repo.remotes() {
        Ok(r)  => r,
        Err(e) => return TagOutcome::Err(format!("tag created locally; push skipped — {e}")),
    };
    let remote_name = match remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
    {
        Some(n) => n.to_string(),
        None    => return TagOutcome::Err("tag created locally; push skipped — no remotes configured".into()),
    };
    let refspec = format!("refs/tags/{tag_name}:refs/tags/{tag_name}");
    match crate::git::remote::push(&repo, &remote_name, &refspec, false) {
        Ok(()) => TagOutcome::Ok(format!("{kind} tag pushed to '{remote_name}'")),
        Err(e) => TagOutcome::Err(format!("tag created locally; push to '{remote_name}' failed — {e}")),
    }
}
