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
//! NOT migrated here (left inline in `workspace_commands`, handled by the
//! later emit/seam pass):
//!
//!   - Anything that emits a frontend event directly — every mutation that
//!     calls `emit_registry_changed` (`arbor://registry-changed`) or
//!     `set_active_workspace` (`arbor://workspace-switched`): `create_workspace`,
//!     `update_workspace`, `delete_workspace`, `reorder_workspaces`,
//!     `set_active_workspace`, `add_repo_to_workspace`,
//!     `remove_repo_from_workspace`, `move_repo_between_workspaces`,
//!     `register_repo_path`, `register_pending_repo`, `update_registry_repo`,
//!     `delete_registry_repo`.
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
//! No fire-and-forget plugin hooks fire from the handlers migrated here.

use crate::commands::workspace_commands::{
    probe_one, ExportedBundle, ExportedWorkspace, ExportedWorkspaceGroup, ImportGroupPreview,
    ImportGroupPreviewWorkspace, ImportPreview, ImportPreviewRepo, RepoHealth,
    RepoRegistryEntryWithRoot, WorkspaceGroupPatch, WorkspacesSnapshot,
};
use crate::error::AppError;
use crate::ipc::platform;
use crate::workspace::{
    migration, snapshot as snapshot_io, store as store_io, CrossWsTabRef, RepoRegistryEntry,
    TabMeta, TabSnapshot, WorkspaceDef, WorkspaceGroup,
};
use crate::AppState;

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
