//! `workspace` domain — platform handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[platform::handler(program = "platform")]` self-registers it under its own
//! function name. The workspace store / repo registry / snapshot I/O already
//! live in the reusable [`crate::workspace`] modules, so handlers delegate to
//! the same store logic — behavior (locks held, store saves, errors) is
//! byte-identical.
//!
//! The aggregate DTOs (`WorkspacesSnapshot`, `ExportedWorkspace`, `ImportPreview`,
//! …) still live in [`crate::commands::workspace_commands`] because the deferred
//! commands there share them; the migrated handlers import them from that module
//! so the serde shape the FE decodes is the same regardless of routing.
//!
//! The registry/workspace mutations whose only egress is `arbor://registry-changed`
//! plus a *reconstructable* fire-and-forget hook are migrated here too: the emit
//! goes through the backend **event sink** ([`AppState::event_sink`] →
//! [`emit_registry_changed`]) and any hook moves out of the handler into the
//! platform `post_hooks` table (so it fires exactly once whether the method is
//! served in-process or, eventually, out-of-process). Migrated:
//! `create_workspace`, `update_workspace`, `reorder_workspaces`,
//! `add_repo_to_workspace`, `move_repo_between_workspaces`, `register_repo_path`,
//! `register_pending_repo`, `update_registry_repo`.
//!
//! NOT migrated here (left inline in `workspace_commands`):
//!
//!   - The three import *commits* (`import_workspace_commit`,
//!     `import_workspace_group_commit`, `import_bundle_commit`): each fires a
//!     **variable number** of `on_workspace_created` / `on_workspace_updated`
//!     hooks (one per touched workspace, created-vs-merged decided from internal
//!     state), with payloads that are NOT reconstructable from params+result —
//!     so the single-fire `post_hooks` seam can't carry them. Deferred until the
//!     seam grows multi-hook support.
//!   - The fetch/pull/tag-all runners (`workspace_fetch_all`,
//!     `workspace_pull_all`, `workspace_tag_all`): take `AppHandle`, spawn a
//!     background thread, and stream `arbor://job-*` / `arbor://workspace-*`
//!     progress events.
//!
//! The fire-and-forget workspace hooks the migrated mutations used to fire
//! inline (`on_workspace_created` / `_updated` / `_repo_added` / `_repo_removed`)
//! now live in the platform `post_hooks` table, reconstructed from params and
//! the handler's return value — the handlers here fire none themselves.
//!
//! Note on which mutations stayed deferred for hook reasons: the ones whose hook
//! payload is NOT reconstructable from params+result — `delete_workspace` and
//! `set_active_workspace` capture pre-mutation store state (the deleted
//! workspace's full payload, the previous active id), and
//! `remove_repo_from_workspace` / `delete_registry_repo` fire
//! `on_repo_deregistered` conditionally off internal GC state — stay inline in
//! `workspace_commands` (the single-fire post_hooks seam can't carry them).

use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;

use crate::commands::workspace_commands::{
    forget_recent_repo, forget_repo_if_orphaned, probe_one, ExportedBundle, ExportedRepo,
    ExportedWorkspace, ExportedWorkspaceGroup, ImportBundleResult, ImportGroupPreview,
    ImportGroupPreviewWorkspace, ImportGroupWorkspaceCommit, ImportPreview, ImportPreviewRepo,
    RepoHealth, RepoRegistrationResult, RepoRegistryEntryWithRoot, WorkspaceFetchStartResult,
    WorkspaceGroupPatch, WorkspacePatch, WorkspacesSnapshot,
};
use crate::error::AppError;
use crate::ipc::platform;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};
use crate::workspace::{
    migration, registry as registry_io, snapshot as snapshot_io, store as store_io, CrossWsTabRef,
    RepoRegistryEntry, TabMeta, TabSnapshot, WorkspaceDef, WorkspaceGroup, SCRATCH_ID,
};
use crate::AppState;

/// Broadcast that the repo registry and/or workspace membership changed, so
/// every window (the main app AND any standalone File Explorer window) can
/// reload its Projects view. Carries no payload — listeners re-query. Routes
/// through the backend event sink (the Model-D-safe egress).
fn emit_registry_changed(state: &AppState) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    sink.emit("arbor://registry-changed", serde_json::Value::Null);
    Ok(())
}

// ---------------------------------------------------------------------------
// Migration report
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn take_migration_report(state: &AppState) -> Result<Option<migration::MigrationReport>, AppError> {
    let mut slot = state
        .migration_report
        .lock()
        .map_err(|_| AppError::MutexPoisoned("migration_report".into()))?;
    Ok(slot.take())
}

// ---------------------------------------------------------------------------
// Query commands
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn list_workspaces(state: &AppState) -> Result<WorkspacesSnapshot, AppError> {
    let store = state.lock_workspaces()?;
    Ok(WorkspacesSnapshot {
        workspaces: store.ordered(),
        groups: {
            let mut g = store.groups.clone();
            g.sort_by_key(|g| (g.order, g.name.to_lowercase()));
            g
        },
        active_workspace_id: store.active_workspace_id.clone(),
    })
}

#[platform::handler(program = "platform")]
fn list_registry_repos(state: &AppState) -> Result<Vec<RepoRegistryEntry>, AppError> {
    let reg = state.lock_repo_registry()?;
    Ok(reg.list())
}

#[platform::handler(program = "platform")]
fn list_registry_with_roots(
    state: &AppState,
) -> Result<Vec<RepoRegistryEntryWithRoot>, AppError> {
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
            current_branch = repo
                .head()
                .ok()
                .and_then(|h| h.shorthand().map(|s| s.to_string()));
            is_worktree = repo.is_worktree();
        }
        out.push(RepoRegistryEntryWithRoot {
            id: e.id,
            path: e.path,
            remote_url: e.remote_url,
            display_name: e.display_name,
            common_dir,
            current_branch,
            is_worktree,
        });
    }
    Ok(out)
}

// `load_workspace_snapshot` never touched `AppState`, but the handler macro
// requires a context first arg, so it takes `_state: &AppState` and ignores it.
#[platform::handler(program = "platform")]
fn load_workspace_snapshot(
    _state: &AppState,
    workspace_id: String,
) -> Result<TabSnapshot, AppError> {
    Ok(snapshot_io::load(&workspace_id))
}

// ---------------------------------------------------------------------------
// Mutation commands — workspaces. Each emits `arbor://registry-changed` via the
// backend event sink; the fire-and-forget workspace hooks they used to fire
// inline now live in the platform `post_hooks` table (reconstructed from
// params+result). `delete_workspace` / `set_active_workspace` stay inline in
// `workspace_commands` (pre-mutation-state hook payloads, not reconstructable).
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn create_workspace(
    state: &AppState,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
) -> Result<WorkspaceDef, AppError> {
    let ws = {
        let mut store = state.lock_workspaces()?;
        let ws = store.create(name, color_idx, repo_ids, group_id);
        store_io::save(&store)?;
        ws
    };
    // Hook `on_workspace_created` now fires from `post_hooks` (payload = R).
    emit_registry_changed(state)?;
    Ok(ws)
}

#[platform::handler(program = "platform")]
fn update_workspace(
    state: &AppState,
    workspace_id: String,
    patch: WorkspacePatch,
) -> Result<WorkspaceDef, AppError> {
    let ws = {
        let mut store = state.lock_workspaces()?;
        {
            let ws = store
                .get_mut(&workspace_id)
                .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
            if let Some(name) = patch.name {
                ws.name = name;
            }
            if let Some(color) = patch.color_idx {
                ws.color_idx = color;
            }
            if let Some(group) = patch.group_id {
                ws.group_id = group.filter(|s| !s.is_empty());
            }
            if let Some(ids) = patch.repo_ids {
                // Dedupe — the management modal's keyed-each can't render
                // the same id twice, and the dropdown count would lie.
                let mut seen = std::collections::HashSet::new();
                ws.repo_ids = ids.into_iter().filter(|id| seen.insert(id.clone())).collect();
            }
        }
        store_io::save(&store)?;
        store
            .get(&workspace_id)
            .cloned()
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?
    };
    // Hook `on_workspace_updated` now fires from `post_hooks` (payload = R).
    emit_registry_changed(state)?;
    Ok(ws)
}

#[platform::handler(program = "platform")]
fn reorder_workspaces(state: &AppState, ordered_ids: Vec<String>) -> Result<(), AppError> {
    {
        let mut store = state.lock_workspaces()?;
        store.set_order(&ordered_ids);
        store_io::save(&store)?;
    }
    emit_registry_changed(state)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn add_repo_to_workspace(
    state: &AppState,
    workspace_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    {
        let mut store = state.lock_workspaces()?;
        store.add_repo(&workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    // Hook `on_workspace_repo_added` now fires from `post_hooks` (payload = P).
    emit_registry_changed(state)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn move_repo_between_workspaces(
    state: &AppState,
    from_workspace_id: String,
    to_workspace_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    {
        let mut store = state.lock_workspaces()?;
        store.remove_repo(&from_workspace_id, &repo_id)?;
        store.add_repo(&to_workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    // Hooks `on_workspace_repo_removed` (from) + `on_workspace_repo_added` (to)
    // now fire from `post_hooks` (both payloads = P).
    emit_registry_changed(state)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Repo registry — registration + editing. (`delete_registry_repo` stays inline:
// it fires `on_repo_deregistered` with pre-removal path/name, not
// reconstructable from params+result.)
// ---------------------------------------------------------------------------

/// Upsert a repo path into the registry AND auto-add it to the active
/// workspace if it isn't already a member of it.
#[platform::handler(program = "platform")]
fn register_repo_path(
    state: &AppState,
    path: String,
    remote_url: Option<String>,
    display_name: Option<String>,
) -> Result<RepoRegistrationResult, AppError> {
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
    let remote_url =
        remote_url.or_else(|| crate::git::url::probe_origin_url(std::path::Path::new(&path)));
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
        let active = store
            .active_workspace_id
            .clone()
            .unwrap_or_else(|| SCRATCH_ID.to_string());
        let ws = store
            .get_mut(&active)
            .ok_or_else(|| AppError::Other(format!("active workspace not found: {active}")))?;
        if ws.repo_ids.iter().any(|i| i == &id) {
            false
        } else {
            ws.repo_ids.push(id.clone());
            store_io::save(&store)?;
            true
        }
    };
    emit_registry_changed(state)?;
    Ok(RepoRegistrationResult { id, existed, added_to_ws })
}

/// Create a "pending" registry entry for a repo that's declared (name +
/// optional remote URL) but not yet on disk — used by the non-blocking
/// workspace import.  Returns the new id.
#[platform::handler(program = "platform")]
fn register_pending_repo(
    state: &AppState,
    name: String,
    remote_url: Option<String>,
) -> Result<String, AppError> {
    let id = {
        let mut reg = state.lock_repo_registry()?;
        let id = reg.insert_pending(remote_url, &name);
        registry_io::save(&reg)?;
        id
    };
    emit_registry_changed(state)?;
    Ok(id)
}

#[platform::handler(program = "platform")]
fn update_registry_repo(
    state: &AppState,
    repo_id: String,
    display_name: Option<String>,
    remote_url: Option<Option<String>>,
    path: Option<String>,
) -> Result<RepoRegistryEntry, AppError> {
    let entry = {
        let mut reg = state.lock_repo_registry()?;
        if let Some(name) = display_name {
            reg.set_display_name(&repo_id, name)?;
        }
        if let Some(url) = remote_url {
            reg.set_remote_url(&repo_id, url)?;
        }
        if let Some(p) = path {
            reg.set_path(&repo_id, p)?;
        }
        registry_io::save(&reg)?;
        reg.get(&repo_id)
            .cloned()
            .ok_or_else(|| AppError::Other(format!("repo not found: {repo_id}")))?
    };
    emit_registry_changed(state)?;
    Ok(entry)
}

// ---------------------------------------------------------------------------
// Mutation commands — groups (no FE event, no hook → leaf-clean)
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn create_workspace_group(
    state: &AppState,
    name: String,
    color_idx: u8,
) -> Result<WorkspaceGroup, AppError> {
    let mut store = state.lock_workspaces()?;
    let g = store.create_group(name, color_idx);
    store_io::save(&store)?;
    Ok(g)
}

#[platform::handler(program = "platform")]
fn update_workspace_group(
    state: &AppState,
    group_id: String,
    patch: WorkspaceGroupPatch,
) -> Result<WorkspaceGroup, AppError> {
    let mut store = state.lock_workspaces()?;
    {
        let g = store
            .get_group_mut(&group_id)
            .ok_or_else(|| AppError::Other(format!("group not found: {group_id}")))?;
        if let Some(name) = patch.name {
            g.name = name;
        }
        if let Some(color) = patch.color_idx {
            g.color_idx = color;
        }
        if let Some(col) = patch.collapsed {
            g.collapsed = col;
        }
    }
    store_io::save(&store)?;
    store
        .get_group(&group_id)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("group not found: {group_id}")))
}

#[platform::handler(program = "platform")]
fn delete_workspace_group(state: &AppState, group_id: String) -> Result<(), AppError> {
    let mut store = state.lock_workspaces()?;
    store.remove_group(&group_id)?;
    store_io::save(&store)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn reorder_workspace_groups(
    state: &AppState,
    ordered_ids: Vec<String>,
) -> Result<(), AppError> {
    let mut store = state.lock_workspaces()?;
    store.set_group_order(&ordered_ids);
    store_io::save(&store)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn set_workspace_group(
    state: &AppState,
    workspace_id: String,
    group_id: Option<String>,
) -> Result<(), AppError> {
    let mut store = state.lock_workspaces()?;
    store.set_workspace_group(&workspace_id, group_id)?;
    store_io::save(&store)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tab snapshots — frontend owns tab state and pushes the full snapshot.
// No `AppState` needed; takes `_state` to satisfy the handler macro.
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn save_workspace_snapshot(
    _state: &AppState,
    workspace_id: String,
    open_tab_ids: Vec<String>,
    active_tab_id: Option<String>,
    cross_ws_tabs: Vec<CrossWsTabRef>,
    tab_meta: Option<Vec<TabMeta>>,
) -> Result<(), AppError> {
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
// Import / export — single workspace (queries only; the commit fires hooks
// and stays inline).
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn export_workspace(state: &AppState, workspace_id: String) -> Result<ExportedWorkspace, AppError> {
    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let ws = store
        .get(&workspace_id)
        .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
    let repos = ws
        .repo_ids
        .iter()
        .filter_map(|id| reg.get(id))
        .map(|e| crate::commands::workspace_commands::ExportedRepo {
            name: e.display_name.clone(),
            remote_url: e.remote_url.clone(),
        })
        .collect();
    Ok(ExportedWorkspace {
        arbor_workspace_version: 1,
        name: ws.name.clone(),
        color_idx: ws.color_idx,
        repos,
    })
}

#[platform::handler(program = "platform")]
fn import_workspace_preview(
    state: &AppState,
    payload: ExportedWorkspace,
) -> Result<ImportPreview, AppError> {
    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let existing_workspace_id = store
        .find_by_name_in_group(&payload.name, None)
        .map(|w| w.id.clone());
    let repos = payload
        .repos
        .into_iter()
        .map(|r| {
            let matched = r
                .remote_url
                .as_deref()
                .and_then(|u| reg.find_by_remote_url(u));
            ImportPreviewRepo {
                existing_id: matched.map(|e| e.id.clone()),
                existing_path: matched.map(|e| e.path.clone()),
                name: r.name,
                remote_url: r.remote_url,
            }
        })
        .collect();
    Ok(ImportPreview {
        name: payload.name,
        color_idx: payload.color_idx,
        existing_workspace_id,
        repos,
    })
}

// ---------------------------------------------------------------------------
// Import / export — workspace GROUPS (queries only).
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn export_workspace_group(
    state: &AppState,
    group_id: String,
) -> Result<ExportedWorkspaceGroup, AppError> {
    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let group = store
        .get_group(&group_id)
        .ok_or_else(|| AppError::Other(format!("group not found: {group_id}")))?;
    // Child workspaces in dropdown order. Scratch can never live in a group,
    // so no explicit filter for it is needed.
    let mut members: Vec<&WorkspaceDef> = store
        .workspaces
        .iter()
        .filter(|w| w.group_id.as_deref() == Some(group_id.as_str()))
        .collect();
    members.sort_by_key(|w| (w.order, w.name.to_lowercase()));
    let workspaces = members
        .into_iter()
        .map(|ws| {
            let repos = ws
                .repo_ids
                .iter()
                .filter_map(|id| reg.get(id))
                .map(|e| crate::commands::workspace_commands::ExportedRepo {
                    name: e.display_name.clone(),
                    remote_url: e.remote_url.clone(),
                })
                .collect();
            crate::commands::workspace_commands::ExportedGroupMember {
                name: ws.name.clone(),
                color_idx: ws.color_idx,
                repos,
            }
        })
        .collect();
    Ok(ExportedWorkspaceGroup {
        arbor_workspace_group_version: 1,
        name: group.name.clone(),
        color_idx: group.color_idx,
        workspaces,
    })
}

#[platform::handler(program = "platform")]
fn import_workspace_group_preview(
    state: &AppState,
    payload: ExportedWorkspaceGroup,
) -> Result<ImportGroupPreview, AppError> {
    use crate::commands::workspace_commands::ExportedRepo;

    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;

    let existing_group_id = store
        .groups
        .iter()
        .find(|g| g.name.eq_ignore_ascii_case(payload.name.trim()))
        .map(|g| g.id.clone());

    let dedup_key = |r: &ExportedRepo| -> String {
        match r.remote_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => format!("url:{}", u.to_lowercase()),
            None => format!("name:{}", r.name.trim().to_lowercase()),
        }
    };

    let mut repos: Vec<ImportPreviewRepo> = Vec::new();
    let mut key_to_idx: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut workspaces = Vec::with_capacity(payload.workspaces.len());

    for ws in &payload.workspaces {
        let mut repo_indices: Vec<usize> = Vec::with_capacity(ws.repos.len());
        for r in &ws.repos {
            let key = dedup_key(r);
            let idx = *key_to_idx.entry(key).or_insert_with(|| {
                let matched = r
                    .remote_url
                    .as_deref()
                    .map(str::trim)
                    .filter(|u| !u.is_empty())
                    .and_then(|u| reg.find_by_remote_url(u));
                let i = repos.len();
                repos.push(ImportPreviewRepo {
                    existing_id: matched.map(|e| e.id.clone()),
                    existing_path: matched.map(|e| e.path.clone()),
                    name: r.name.clone(),
                    remote_url: r.remote_url.clone(),
                });
                i
            });
            // A workspace may list the same repo twice; keep its row once.
            if !repo_indices.contains(&idx) {
                repo_indices.push(idx);
            }
        }
        // Only meaningful when we'd merge into an existing group: a same-named
        // member already in it is updated rather than duplicated.
        let existing_workspace_id = existing_group_id
            .as_deref()
            .and_then(|gid| store.find_by_name_in_group(&ws.name, Some(gid)))
            .map(|w| w.id.clone());
        workspaces.push(ImportGroupPreviewWorkspace {
            name: ws.name.clone(),
            color_idx: ws.color_idx,
            repo_indices,
            existing_workspace_id,
        });
    }

    Ok(ImportGroupPreview {
        name: payload.name,
        color_idx: payload.color_idx,
        existing_group_id,
        repos,
        workspaces,
    })
}

// ---------------------------------------------------------------------------
// Full backup export — every group (with members) + every top-level workspace.
// Scratch is excluded (ephemeral catch-all, not a curated workspace).
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn export_all_workspaces(state: &AppState) -> Result<ExportedBundle, AppError> {
    use crate::commands::workspace_commands::{ExportedGroupMember, ExportedRepo};
    use crate::workspace::SCRATCH_ID;

    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let repos_of = |ws: &WorkspaceDef| -> Vec<ExportedRepo> {
        ws.repo_ids
            .iter()
            .filter_map(|id| reg.get(id))
            .map(|e| ExportedRepo {
                name: e.display_name.clone(),
                remote_url: e.remote_url.clone(),
            })
            .collect()
    };
    // Groups (sorted) each carrying their member workspaces (sorted).
    let mut sorted_groups = store.groups.clone();
    sorted_groups.sort_by_key(|g| (g.order, g.name.to_lowercase()));
    let groups = sorted_groups
        .iter()
        .map(|g| {
            let mut members: Vec<&WorkspaceDef> = store
                .workspaces
                .iter()
                .filter(|w| w.group_id.as_deref() == Some(g.id.as_str()))
                .collect();
            members.sort_by_key(|w| (w.order, w.name.to_lowercase()));
            ExportedWorkspaceGroup {
                arbor_workspace_group_version: 1,
                name: g.name.clone(),
                color_idx: g.color_idx,
                workspaces: members
                    .into_iter()
                    .map(|ws| ExportedGroupMember {
                        name: ws.name.clone(),
                        color_idx: ws.color_idx,
                        repos: repos_of(ws),
                    })
                    .collect(),
            }
        })
        .collect();
    // Top-level (ungrouped) workspaces, excluding Scratch.
    let mut top: Vec<&WorkspaceDef> = store
        .workspaces
        .iter()
        .filter(|w| w.group_id.is_none() && w.id != SCRATCH_ID)
        .collect();
    top.sort_by_key(|w| (w.order, w.name.to_lowercase()));
    let workspaces = top
        .into_iter()
        .map(|ws| ExportedWorkspace {
            arbor_workspace_version: 1,
            name: ws.name.clone(),
            color_idx: ws.color_idx,
            repos: repos_of(ws),
        })
        .collect();
    Ok(ExportedBundle {
        arbor_workspace_bundle_version: 1,
        groups,
        workspaces,
    })
}

// ---------------------------------------------------------------------------
// Health scan — lightweight per-repo status (branch + ahead/behind + dirty).
//
// The original command was `async fn` but its body is fully synchronous (no
// `.await`); it was only `async` so Tauri scheduled it off the main thread. The
// broker already dispatches off the UI thread, so it drops to a plain handler —
// behavior (snapshot under lock, probe lock-free) is unchanged.
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn workspace_health_scan(
    state: &AppState,
    workspace_id: String,
) -> Result<Vec<RepoHealth>, AppError> {
    // Snapshot the list so we don't hold any locks while probing (each
    // Repository::open can do significant I/O).
    let entries: Vec<RepoRegistryEntry> = {
        let store = state.lock_workspaces()?;
        let reg = state.lock_repo_registry()?;
        let ws = store
            .get(&workspace_id)
            .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
        ws.repo_ids
            .iter()
            .filter_map(|id| reg.get(id).cloned())
            .collect()
    };
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        out.push(probe_one(&e));
    }
    Ok(out)
}

// ===========================================================================
// emit/seam pass — mutations + background runners moved off `AppHandle`.
//
// These fire their workspace hooks **inline** (`state.fire_hook`) rather than
// through the platform `post_hooks` table: their payloads capture pre-mutation
// store state or a variable number of touched workspaces, which the single-fire
// post_hooks seam can't reconstruct. They are NOT registered in `post_hooks`, so
// the inline fire is the only one — no double-fire. FE events go through the
// backend event sink (`state.emit` / `emit_registry_changed`); the background
// runners carry an `Arc<dyn EventSink>` + the shared `Arc<Mutex<JobRegistry>>`
// into their worker thread instead of an `AppHandle`.
// ===========================================================================

fn workspace_payload(ws: &WorkspaceDef) -> serde_json::Value {
    serde_json::json!({
        "id":         ws.id,
        "name":       ws.name,
        "color_idx":  ws.color_idx,
        "repo_ids":   ws.repo_ids,
        "group_id":   ws.group_id,
        "repo_count": ws.repo_ids.len(),
    })
}

// ── Workspace mutations (pre-mutation-state hooks → inline) ──────────────────

#[platform::handler(program = "platform")]
fn delete_workspace(state: &AppState, workspace_id: String) -> Result<(), AppError> {
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
        state.fire_hook("on_workspace_deleted", payload);
    }
    // Forget every member that's no longer referenced by another workspace, so
    // Arbor stops proposing it as "use existing" on a later import.
    for repo_id in member_ids {
        let _ = forget_repo_if_orphaned(state, &repo_id, "workspace_deleted");
    }
    emit_registry_changed(state)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn set_active_workspace(state: &AppState, workspace_id: String) -> Result<WorkspaceDef, AppError> {
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
    state.emit("arbor://workspace-switched", &payload);
    state.fire_hook("on_workspace_switched", payload);
    Ok(ws)
}

#[platform::handler(program = "platform")]
fn remove_repo_from_workspace(
    state: &AppState,
    workspace_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    {
        let mut store = state.lock_workspaces()?;
        store.remove_repo(&workspace_id, &repo_id)?;
        store_io::save(&store)?;
    }
    state.fire_hook("on_workspace_repo_removed", serde_json::json!({
        "workspace_id": workspace_id,
        "repo_id":      repo_id,
    }));
    // If that was the repo's last workspace, forget it entirely.
    let _ = forget_repo_if_orphaned(state, &repo_id, "removed_from_last_workspace");
    emit_registry_changed(state)?;
    Ok(())
}

#[platform::handler(program = "platform")]
fn delete_registry_repo(state: &AppState, repo_id: String) -> Result<(), AppError> {
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
        let _ = forget_recent_repo(state, &path);
        state.fire_hook("on_repo_deregistered", serde_json::json!({
            "repo_id": repo_id,
            "path":    path,
            "name":    name,
            "reason":  "registry_delete",
        }));
    }
    emit_registry_changed(state)?;
    Ok(())
}

// ── Import commits (variable-count hooks → inline) ───────────────────────────

#[platform::handler(program = "platform")]
fn import_workspace_commit(
    state: &AppState,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
    merge_into: Option<String>,
) -> Result<WorkspaceDef, AppError> {
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
    state.fire_hook(
        if merged { "on_workspace_updated" } else { "on_workspace_created" },
        workspace_payload(&ws),
    );
    Ok(ws)
}

#[platform::handler(program = "platform")]
fn import_workspace_group_commit(
    state: &AppState,
    name: String,
    color_idx: u8,
    existing_group_id: Option<String>,
    workspaces: Vec<ImportGroupWorkspaceCommit>,
) -> Result<(), AppError> {
    // (workspace, was_merged) for each member, to fire the right hook after.
    let touched: Vec<(WorkspaceDef, bool)> = {
        let mut store = state.lock_workspaces()?;
        let (group_id, merged_group) = match existing_group_id.filter(|id| store.get_group(id).is_some()) {
            Some(id) => (id, true),
            None     => (store.create_group(name, color_idx).id, false),
        };
        let mut touched = Vec::with_capacity(workspaces.len());
        for w in workspaces {
            // Merge only when reusing the existing group AND the target still exists.
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
        state.fire_hook(
            if *merged { "on_workspace_updated" } else { "on_workspace_created" },
            workspace_payload(ws),
        );
    }
    Ok(())
}

#[platform::handler(program = "platform")]
fn import_bundle_commit(
    state: &AppState,
    payload: ExportedBundle,
) -> Result<ImportBundleResult, AppError> {
    // Dedup key: prefer the remote URL, fall back to the display name.
    fn dedup_key(r: &ExportedRepo) -> String {
        match r.remote_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => format!("url:{}", u.to_lowercase()),
            None    => format!("name:{}", r.name.trim().to_lowercase()),
        }
    }

    let mut result = ImportBundleResult::default();

    // Pass 1 — resolve every distinct repo across the bundle to a registry id.
    let mut key_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    {
        let mut reg = state.lock_repo_registry()?;
        let mut all: Vec<&ExportedRepo> = Vec::new();
        for g in &payload.groups { for w in &g.workspaces { all.extend(w.repos.iter()); } }
        for w in &payload.workspaces { all.extend(w.repos.iter()); }
        for r in all {
            let key = dedup_key(r);
            if key_to_id.contains_key(&key) { continue; }
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
        state.fire_hook(if *merged { "on_workspace_updated" } else { "on_workspace_created" }, workspace_payload(ws));
    }
    Ok(result)
}

// ── Background runners (fetch / pull / tag) — EventSink + JobRegistry ─────────

/// Emit through the backend sink when present (it always is once the backend is
/// wired; `None` only in a not-yet-initialised window, where dropping is safe).
fn sink_emit(sink: &Option<Arc<dyn EventSink>>, topic: &str, payload: serde_json::Value) {
    if let Some(s) = sink {
        s.emit(topic, payload);
    }
}

/// Append a line to the job's output buffer and mirror it to the Jobs overlay.
fn log_and_emit(
    sink: &Option<Arc<dyn EventSink>>,
    jobs: &Arc<Mutex<JobRegistry>>,
    job_id: &str,
    line: &str,
) {
    if let Ok(mut j) = jobs.lock() {
        j.append_output(job_id, line.to_string());
    }
    sink_emit(sink, "arbor://job-output", serde_json::json!({
        "job_id": job_id,
        "text":   line,
    }));
}

/// Register a system Job for a workspace-wide run and emit `arbor://job-started`.
/// Returns the new job id.
fn start_workspace_job(
    state: &AppState,
    job_name: &str,
    job_cmd: &str,
) -> Result<String, AppError> {
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            job_name.to_string(),
            plugin_name:     "arbor".into(),
            command:         job_cmd.to_string(),
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
    // `upsertJob` overwrites the registry row with `name = undefined`.
    state.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        job_name,
        "plugin_name": "arbor",
        "command":     job_cmd,
        "category":    "System",
    }));
    Ok(job_id)
}

/// Freeze the (repo_id, path, display_name) targets of a workspace's existing
/// repos under the locks, then release them before the (slow) run.
fn workspace_targets(
    state: &AppState,
    workspace_id: &str,
) -> Result<Vec<(String, String, String)>, AppError> {
    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let ws = store
        .get(workspace_id)
        .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
    Ok(ws.repo_ids.iter()
        .filter_map(|id| reg.get(id))
        .filter(|e| std::path::Path::new(&e.path).exists())
        .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
        .collect())
}

#[platform::handler(program = "platform")]
fn workspace_fetch_all(
    state: &AppState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = format!("Fetch workspace ({total} repos)");
    let job_cmd  = format!("workspace-fetch-all:{workspace_id}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-fetch-{jid}"))
        .spawn(move || {
            let mut ok   = 0usize;
            let mut fail = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match fetch_one(path) {
                    Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {e}"));
                        sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            // Notify the frontend to refresh the graph for the active tab.
            sink_emit(&sink, "arbor://workspace-fetch-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "ok": ok, "failed": fail,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn fetch thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

fn fetch_one(path: &str) -> std::result::Result<String, String> {
    let repo = git2::Repository::open(path).map_err(|e| e.to_string())?;
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    let remote_name = remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| "no remotes configured".to_string())?
        .to_string();
    let res = crate::git::remote::fetch(&repo, &remote_name).map_err(|e| e.to_string())?;
    Ok(format!("remote='{}' objects={} bytes={}", res.remote, res.received_objects, res.received_bytes))
}

#[platform::handler(program = "platform")]
fn workspace_pull_all(
    state: &AppState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = format!("Pull workspace ({total} repos)");
    let job_cmd  = format!("workspace-pull-all:{workspace_id}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-pull-{jid}"))
        .spawn(move || {
            let mut ok       = 0usize;
            let mut fail     = 0usize;
            let mut conflict = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match pull_one(path) {
                    PullOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    PullOutcome::Conflict(msg) => {
                        conflict += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  conflict — {msg}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "conflict", "error": msg,
                        }));
                    }
                    PullOutcome::Err(msg) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {msg}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": msg,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {conflict} conflict, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 && conflict == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": exit_code == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink_emit(&sink, "arbor://workspace-pull-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "ok": ok, "failed": fail, "conflict": conflict,
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

    // Refuse detached HEAD up front — a clear message lets the UI suggest
    // checking out a branch first.
    if let Ok(head) = repo.head() {
        if !head.is_branch() {
            return PullOutcome::Err("detached HEAD — check out a branch to pull".into());
        }
    }

    // Already mid-operation: surface as a conflict so the user knows this repo
    // needs attention before the next run.
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
            if has_merge(&gitdir) {
                PullOutcome::Conflict(e.to_string())
            } else {
                PullOutcome::Err(e.to_string())
            }
        }
    }
}

#[platform::handler(program = "platform")]
fn workspace_tag_all(
    state: &AppState,
    workspace_id: String,
    tag_name: String,
    message: Option<String>,
    push: bool,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let trimmed = tag_name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Other("tag name is required".into()));
    }

    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = if push {
        format!("Tag workspace '{trimmed}' + push ({total} repos)")
    } else {
        format!("Tag workspace '{trimmed}' ({total} repos)")
    };
    let job_cmd = format!("workspace-tag-all:{workspace_id}:{trimmed}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    let tag   = trimmed.clone();
    let msg   = message.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-tag-{jid}"))
        .spawn(move || {
            let mut ok      = 0usize;
            let mut fail    = 0usize;
            let mut skipped = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match tag_one(path, &tag, msg.as_deref(), push) {
                    TagOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    TagOutcome::Skipped(reason) => {
                        skipped += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  skipped — {reason}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "skipped", "error": reason,
                        }));
                    }
                    TagOutcome::Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {e}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {skipped} skipped, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink_emit(&sink, "arbor://workspace-tag-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "tag_name": tag,
                "ok": ok, "failed": fail, "skipped": skipped,
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
