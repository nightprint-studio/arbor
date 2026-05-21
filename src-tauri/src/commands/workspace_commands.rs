use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::AppState;
use crate::error::{AppError, Result};
use crate::jobs::{JobInfo, JobStatus, JobRegistry};
use crate::workspace::{
    migration, registry as registry_io, snapshot as snapshot_io, store as store_io,
    CrossWsTabRef, RepoRegistryEntry, TabMeta, TabSnapshot, WorkspaceDef, WorkspaceGroup,
    SCRATCH_ID,
};

// ---------------------------------------------------------------------------
// Migration — reports the repos that were ingested from the legacy
// session.json at startup.  The welcome screen reads this once; on the
// first call we return the stored report and clear it so subsequent launches
// see nothing.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn take_migration_report(state: State<'_, AppState>) -> Result<Option<migration::MigrationReport>> {
    let mut slot = state.migration_report.lock()
        .map_err(|_| AppError::MutexPoisoned("migration_report".into()))?;
    Ok(slot.take())
}

// ---------------------------------------------------------------------------
// Hook helpers — workspace events reach plugins via the regular hook pipe.
// Each payload is kept minimal: plugins that need more should query back via
// arbor.workspace.* APIs (Phase 7) so we avoid schema drift here.
// ---------------------------------------------------------------------------

fn fire_hook(app: &AppHandle, hook: &str, payload: serde_json::Value) {
    let state = app.state::<AppState>();
    let Ok(host) = state.lock_plugin_host() else { return; };
    let _ = host.fire_hook(hook, &payload.to_string());
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
// Query commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_workspaces(state: State<'_, AppState>) -> Result<WorkspacesSnapshot> {
    let store = state.lock_workspaces()?;
    Ok(WorkspacesSnapshot {
        workspaces:          store.ordered(),
        groups:              {
            let mut g = store.groups.clone();
            g.sort_by_key(|g| (g.order, g.name.to_lowercase()));
            g
        },
        active_workspace_id: store.active_workspace_id.clone(),
    })
}

#[tauri::command]
pub fn list_registry_repos(state: State<'_, AppState>) -> Result<Vec<RepoRegistryEntry>> {
    let reg = state.lock_repo_registry()?;
    Ok(reg.list())
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

#[tauri::command]
pub fn list_registry_with_roots(state: State<'_, AppState>) -> Result<Vec<RepoRegistryEntryWithRoot>> {
    let entries = {
        let reg = state.lock_repo_registry()?;
        reg.list()
    };
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        let mut common_dir: Option<String> = None;
        let mut current_branch: Option<String> = None;
        let mut is_worktree = false;
        if let Ok(repo) = git2::Repository::open(&e.path) {
            common_dir = std::fs::canonicalize(repo.commondir()).ok().map(|p| {
                let s = p.to_string_lossy().to_string();
                let s = s.strip_prefix(r"\\?\").map(|x| x.to_string()).unwrap_or(s);
                s.replace('\\', "/").trim_end_matches('/').to_string()
            });
            current_branch = repo.head().ok()
                .and_then(|h| h.shorthand().map(|s| s.to_string()));
            is_worktree = repo.is_worktree();
        }
        out.push(RepoRegistryEntryWithRoot {
            id:             e.id,
            path:           e.path,
            remote_url:     e.remote_url,
            display_name:   e.display_name,
            common_dir,
            current_branch,
            is_worktree,
        });
    }
    Ok(out)
}

#[tauri::command]
pub fn load_workspace_snapshot(workspace_id: String) -> Result<TabSnapshot> {
    Ok(snapshot_io::load(&workspace_id))
}

// ---------------------------------------------------------------------------
// Mutation commands — workspaces
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
) -> Result<WorkspaceDef> {
    let ws = {
        let mut store = state.lock_workspaces()?;
        let ws = store.create(name, color_idx, repo_ids, group_id);
        store_io::save(&store)?;
        ws
    };
    fire_hook(&app, "on_workspace_created", workspace_payload(&ws));
    Ok(ws)
}

#[derive(Debug, Deserialize)]
pub struct WorkspacePatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub group_id:  Option<Option<String>>, // double-option lets null clear the group
    pub repo_ids:  Option<Vec<String>>,
}

#[tauri::command]
pub fn update_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    patch: WorkspacePatch,
) -> Result<WorkspaceDef> {
    let ws = {
        let mut store = state.lock_workspaces()?;
        {
            let ws = store.get_mut(&workspace_id)
                .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
            if let Some(name)  = patch.name      { ws.name = name; }
            if let Some(color) = patch.color_idx { ws.color_idx = color; }
            if let Some(group) = patch.group_id  { ws.group_id  = group.filter(|s| !s.is_empty()); }
            if let Some(ids)   = patch.repo_ids  {
                // Dedupe — the management modal's keyed-each can't render
                // the same id twice, and the dropdown count would lie.
                let mut seen = std::collections::HashSet::new();
                ws.repo_ids = ids.into_iter().filter(|id| seen.insert(id.clone())).collect();
            }
        }
        store_io::save(&store)?;
        store.get(&workspace_id).cloned()
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?
    };
    fire_hook(&app, "on_workspace_updated", workspace_payload(&ws));
    Ok(ws)
}

#[tauri::command]
pub fn delete_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<()> {
    if workspace_id == SCRATCH_ID {
        return Err(AppError::Other("cannot delete the Scratch workspace".into()));
    }
    let deleted_payload = {
        let store = state.lock_workspaces()?;
        store.get(&workspace_id).map(workspace_payload)
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
    Ok(())
}

#[tauri::command]
pub fn reorder_workspaces(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<()> {
    let mut store = state.lock_workspaces()?;
    store.set_order(&ordered_ids);
    store_io::save(&store)?;
    Ok(())
}

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
// Mutation commands — groups
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn create_workspace_group(
    state: State<'_, AppState>,
    name: String,
    color_idx: u8,
) -> Result<WorkspaceGroup> {
    let mut store = state.lock_workspaces()?;
    let g = store.create_group(name, color_idx);
    store_io::save(&store)?;
    Ok(g)
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceGroupPatch {
    pub name:      Option<String>,
    pub color_idx: Option<u8>,
    pub collapsed: Option<bool>,
}

#[tauri::command]
pub fn update_workspace_group(
    state: State<'_, AppState>,
    group_id: String,
    patch: WorkspaceGroupPatch,
) -> Result<WorkspaceGroup> {
    let mut store = state.lock_workspaces()?;
    {
        let g = store.get_group_mut(&group_id)
            .ok_or_else(|| AppError::Other(format!("group not found: {group_id}")))?;
        if let Some(name)  = patch.name      { g.name = name; }
        if let Some(color) = patch.color_idx { g.color_idx = color; }
        if let Some(col)   = patch.collapsed { g.collapsed = col; }
    }
    store_io::save(&store)?;
    store.get_group(&group_id).cloned()
        .ok_or_else(|| AppError::Other(format!("group not found: {group_id}")))
}

#[tauri::command]
pub fn delete_workspace_group(state: State<'_, AppState>, group_id: String) -> Result<()> {
    let mut store = state.lock_workspaces()?;
    store.remove_group(&group_id)?;
    store_io::save(&store)?;
    Ok(())
}

#[tauri::command]
pub fn reorder_workspace_groups(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<()> {
    let mut store = state.lock_workspaces()?;
    store.set_group_order(&ordered_ids);
    store_io::save(&store)?;
    Ok(())
}

#[tauri::command]
pub fn set_workspace_group(
    state: State<'_, AppState>,
    workspace_id: String,
    group_id: Option<String>,
) -> Result<()> {
    let mut store = state.lock_workspaces()?;
    store.set_workspace_group(&workspace_id, group_id)?;
    store_io::save(&store)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Mutation commands — repo membership
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn add_repo_to_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    repo_id: String,
) -> Result<()> {
    {
        let mut store = state.lock_workspaces()?;
        store.add_repo(&workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    fire_hook(&app, "on_workspace_repo_added", serde_json::json!({
        "workspace_id": workspace_id,
        "repo_id":      repo_id,
    }));
    Ok(())
}

#[tauri::command]
pub fn remove_repo_from_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    workspace_id: String,
    repo_id: String,
) -> Result<()> {
    // Was this the last workspace the repo lived in? Capture before mutating
    // so we can decide whether to fire on_repo_deregistered after the save.
    let was_orphaned = {
        let mut store = state.lock_workspaces()?;
        store.remove_repo(&workspace_id, &repo_id)?;
        let orphaned = !store.repo_is_in_any_workspace(&repo_id);
        store_io::save(&store)?;
        orphaned
    };
    fire_hook(&app, "on_workspace_repo_removed", serde_json::json!({
        "workspace_id": workspace_id,
        "repo_id":      repo_id,
    }));

    // Repo is no longer in any workspace. If it's also not currently open in
    // any tab, treat it as deregistered so plugins can drop their per-repo
    // caches. The registry entry itself stays (the user can re-add it later)
    // — this hook is purely for cache cleanup.
    if was_orphaned {
        let path_name = {
            let reg = state.lock_repo_registry()?;
            reg.get(&repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
        };
        if let Some((path, name)) = path_name {
            let in_open_tab = state.lock_repos()
                .map(|mgr| mgr.all_info().iter().any(|i| i.path == path))
                .unwrap_or(false);
            if !in_open_tab {
                fire_hook(&app, "on_repo_deregistered", serde_json::json!({
                    "repo_id": repo_id,
                    "path":    path,
                    "name":    name,
                    "reason":  "removed_from_last_workspace",
                }));
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn move_repo_between_workspaces(
    app: AppHandle,
    state: State<'_, AppState>,
    from_workspace_id: String,
    to_workspace_id: String,
    repo_id: String,
) -> Result<()> {
    {
        let mut store = state.lock_workspaces()?;
        store.remove_repo(&from_workspace_id, &repo_id)?;
        store.add_repo(&to_workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    fire_hook(&app, "on_workspace_repo_removed", serde_json::json!({
        "workspace_id": from_workspace_id,
        "repo_id":      repo_id,
    }));
    fire_hook(&app, "on_workspace_repo_added", serde_json::json!({
        "workspace_id": to_workspace_id,
        "repo_id":      repo_id,
    }));
    Ok(())
}

// ---------------------------------------------------------------------------
// Repo registry — registration + editing + removal
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct RepoRegistrationResult {
    pub id:           String,
    pub existed:      bool,
    pub added_to_ws:  bool,
}

/// Upsert a repo path into the registry AND auto-add it to the active
/// workspace if it isn't already a member of it.  The boolean fields tell
/// the UI whether a new entry was created and whether we touched the
/// current workspace's membership.
#[tauri::command]
pub fn register_repo_path(
    state: State<'_, AppState>,
    path: String,
    remote_url: Option<String>,
    display_name: Option<String>,
) -> Result<RepoRegistrationResult> {
    let fallback_name = display_name.unwrap_or_else(|| {
        std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repository".to_string())
    });
    // If the caller didn't tell us the remote URL (typical for "Open folder…"
    // and the deep-link clone path), probe `origin` from disk.  Without this
    // the registry entry has `remote_url = None` and the deep-link router
    // can't match `arbor://…?url=…` to this clone — it would fall through to
    // the "needs clone" prompt every time.
    let remote_url = remote_url.or_else(|| crate::git::url::probe_origin_url(std::path::Path::new(&path)));
    let (id, existed) = {
        let mut reg = state.lock_repo_registry()?;
        let existed = reg.find_by_path(&path).is_some();
        let id = reg.upsert_by_path(&path, remote_url, &fallback_name);
        registry_io::save(&reg)?;
        (id, existed)
    };
    // Auto-add to active workspace if missing.
    let added_to_ws = {
        let mut store = state.lock_workspaces()?;
        let active = store.active_workspace_id.clone().unwrap_or_else(|| SCRATCH_ID.to_string());
        let ws = store.get_mut(&active)
            .ok_or_else(|| AppError::Other(format!("active workspace not found: {active}")))?;
        if ws.repo_ids.iter().any(|i| i == &id) { false } else {
            ws.repo_ids.push(id.clone());
            store_io::save(&store)?;
            true
        }
    };
    Ok(RepoRegistrationResult { id, existed, added_to_ws })
}

#[tauri::command]
pub fn update_registry_repo(
    state: State<'_, AppState>,
    repo_id: String,
    display_name: Option<String>,
    remote_url: Option<Option<String>>,
    path: Option<String>,
) -> Result<RepoRegistryEntry> {
    let mut reg = state.lock_repo_registry()?;
    if let Some(name) = display_name { reg.set_display_name(&repo_id, name)?; }
    if let Some(url)  = remote_url   { reg.set_remote_url(&repo_id, url)?; }
    if let Some(p)    = path         { reg.set_path(&repo_id, p)?; }
    registry_io::save(&reg)?;
    reg.get(&repo_id).cloned()
        .ok_or_else(|| AppError::Other(format!("repo not found: {repo_id}")))
}

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
        fire_hook(&app, "on_repo_deregistered", serde_json::json!({
            "repo_id": repo_id,
            "path":    path,
            "name":    name,
            "reason":  "registry_delete",
        }));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tab snapshots — persisted per workspace.  The frontend owns tab state and
// pushes the complete snapshot whenever it changes; we just write it out.
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn save_workspace_snapshot(
    workspace_id: String,
    open_tab_ids: Vec<String>,
    active_tab_id: Option<String>,
    cross_ws_tabs: Vec<CrossWsTabRef>,
    tab_meta: Option<Vec<TabMeta>>,
) -> Result<()> {
    let snap = TabSnapshot {
        open_tab_ids,
        active_tab_id,
        cross_ws_tabs,
        tab_meta: tab_meta.unwrap_or_default(),
    };
    snapshot_io::save(&workspace_id, &snap)?;
    Ok(())
}

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

#[tauri::command]
pub fn export_workspace(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<ExportedWorkspace> {
    let store = state.lock_workspaces()?;
    let reg   = state.lock_repo_registry()?;
    let ws = store.get(&workspace_id)
        .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
    let repos = ws.repo_ids.iter()
        .filter_map(|id| reg.get(id))
        .map(|e| ExportedRepo {
            name:       e.display_name.clone(),
            remote_url: e.remote_url.clone(),
        })
        .collect();
    Ok(ExportedWorkspace {
        arbor_workspace_version: 1,
        name:                    ws.name.clone(),
        color_idx:               ws.color_idx,
        repos,
    })
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
    pub repos:     Vec<ImportPreviewRepo>,
}

#[tauri::command]
pub fn import_workspace_preview(
    state: State<'_, AppState>,
    payload: ExportedWorkspace,
) -> Result<ImportPreview> {
    let reg = state.lock_repo_registry()?;
    let repos = payload.repos.into_iter().map(|r| {
        let matched = r.remote_url.as_deref().and_then(|u| reg.find_by_remote_url(u));
        ImportPreviewRepo {
            existing_id:   matched.map(|e| e.id.clone()),
            existing_path: matched.map(|e| e.path.clone()),
            name:          r.name,
            remote_url:    r.remote_url,
        }
    }).collect();
    Ok(ImportPreview {
        name:      payload.name,
        color_idx: payload.color_idx,
        repos,
    })
}

/// Create a workspace from a list of already-resolved repo ids.  The
/// frontend does the per-repo Locate/Clone/Skip dance and passes us the
/// final list of registry ids to wrap up.
#[tauri::command]
pub fn import_workspace_commit(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
) -> Result<WorkspaceDef> {
    let ws = {
        let mut store = state.lock_workspaces()?;
        let ws = store.create(name, color_idx, repo_ids, group_id);
        store_io::save(&store)?;
        ws
    };
    fire_hook(&app, "on_workspace_created", workspace_payload(&ws));
    Ok(ws)
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

fn probe_one(entry: &RepoRegistryEntry) -> RepoHealth {
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

#[tauri::command]
pub async fn workspace_health_scan(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<RepoHealth>> {
    // Snapshot the list so we don't hold any locks while probing (each
    // Repository::open can do significant I/O).
    let entries: Vec<RepoRegistryEntry> = {
        let store = state.lock_workspaces()?;
        let reg   = state.lock_repo_registry()?;
        let ws = store.get(&workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids.iter().filter_map(|id| reg.get(id).cloned()).collect()
    };
    let mut out = Vec::with_capacity(entries.len());
    for e in entries { out.push(probe_one(&e)); }
    Ok(out)
}

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
