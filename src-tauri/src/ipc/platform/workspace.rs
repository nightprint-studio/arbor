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

use crate::commands::workspace_commands::{
    probe_one, ExportedBundle, ExportedWorkspace, ExportedWorkspaceGroup, ImportGroupPreview,
    ImportGroupPreviewWorkspace, ImportPreview, ImportPreviewRepo, RepoHealth,
    RepoRegistrationResult, RepoRegistryEntryWithRoot, WorkspaceGroupPatch, WorkspacePatch,
    WorkspacesSnapshot,
};
use crate::error::AppError;
use crate::ipc::platform;
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
