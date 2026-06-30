//! `workspace` domain — read-only queries, import/export previews, the full
//! backup export + the health scan, served **out-of-process** by corvus-be.
//!
//! OOP twin of the shell's `crate::ipc::platform::workspace` query handlers.
//! Each handler delegates to the reload-on-access store / registry / snapshot
//! API in [`crate::workspace`] (corvus-be is the authority for these files —
//! ADR-1), runs against [`CorvusState`], and returns the same wire DTOs the FE
//! decodes. None of these fire hooks; the snapshot SAVE here writes the snapshot
//! file only (no hook, no `arbor://registry-changed`). Slow git work (the
//! `entry_with_root` / `probe_one` probes) runs lock-free, after the entries are
//! cloned out from under the store/registry guards — exactly as the shell
//! snapshotted under lock then probed.

use corvus_core::prelude::CorvusState;

use crate::workspace::{
    entry_with_root, probe_one, registry, snapshot, store, CrossWsTabRef, ExportedBundle,
    ExportedGroupMember, ExportedRepo, ExportedWorkspace, ExportedWorkspaceGroup, ImportGroupPreview,
    ImportGroupPreviewWorkspace, ImportPreview, ImportPreviewRepo, RepoHealth, RepoRegistryEntry,
    RepoRegistryEntryWithRoot, TabMeta, TabSnapshot, WorkspaceDef, WorkspacesSnapshot, SCRATCH_ID,
};

// ---------------------------------------------------------------------------
// Query commands
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn list_workspaces(state: &CorvusState) -> Result<WorkspacesSnapshot, String> {
    let store = store::store(state);
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

#[arbor_rpc::handler]
fn list_registry_repos(state: &CorvusState) -> Result<Vec<RepoRegistryEntry>, String> {
    let reg = registry::registry(state);
    Ok(reg.list())
}

#[arbor_rpc::handler]
fn list_registry_with_roots(
    state: &CorvusState,
) -> Result<Vec<RepoRegistryEntryWithRoot>, String> {
    // Snapshot the entries under lock, then open each repo lock-free — each
    // `Repository::open` can do significant I/O.
    let entries = {
        let reg = registry::registry(state);
        reg.list()
    };
    let mut out = Vec::with_capacity(entries.len());
    for e in entries {
        out.push(entry_with_root(e));
    }
    Ok(out)
}

#[arbor_rpc::handler]
fn load_workspace_snapshot(
    state: &CorvusState,
    workspace_id: String,
) -> Result<TabSnapshot, String> {
    Ok(snapshot::load(state, &workspace_id))
}

// ---------------------------------------------------------------------------
// Tab snapshots — frontend owns tab state and pushes the full snapshot. No
// hook, no `arbor://registry-changed`; this only writes the snapshot file.
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn save_workspace_snapshot(
    state: &CorvusState,
    workspace_id: String,
    open_tab_ids: Vec<String>,
    active_tab_id: Option<String>,
    cross_ws_tabs: Vec<CrossWsTabRef>,
    tab_meta: Option<Vec<TabMeta>>,
) -> Result<(), String> {
    let snap = TabSnapshot {
        open_tab_ids,
        active_tab_id,
        cross_ws_tabs,
        tab_meta: tab_meta.unwrap_or_default(),
    };
    snapshot::save(state, &workspace_id, &snap)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Import / export — single workspace (queries only; the commit fires hooks
// and lives in `workspace_mutation`).
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn export_workspace(state: &CorvusState, workspace_id: String) -> Result<ExportedWorkspace, String> {
    let store = store::store(state);
    let reg = registry::registry(state);
    let ws = store
        .get(&workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
    let repos = ws
        .repo_ids
        .iter()
        .filter_map(|id| reg.get(id))
        .map(|e| ExportedRepo {
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

#[arbor_rpc::handler]
fn import_workspace_preview(
    state: &CorvusState,
    payload: ExportedWorkspace,
) -> Result<ImportPreview, String> {
    let store = store::store(state);
    let reg = registry::registry(state);
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

#[arbor_rpc::handler]
fn export_workspace_group(
    state: &CorvusState,
    group_id: String,
) -> Result<ExportedWorkspaceGroup, String> {
    let store = store::store(state);
    let reg = registry::registry(state);
    let group = store
        .get_group(&group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;
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
                .map(|e| ExportedRepo {
                    name: e.display_name.clone(),
                    remote_url: e.remote_url.clone(),
                })
                .collect();
            ExportedGroupMember {
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

#[arbor_rpc::handler]
fn import_workspace_group_preview(
    state: &CorvusState,
    payload: ExportedWorkspaceGroup,
) -> Result<ImportGroupPreview, String> {
    let store = store::store(state);
    let reg = registry::registry(state);

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

#[arbor_rpc::handler]
fn export_all_workspaces(state: &CorvusState) -> Result<ExportedBundle, String> {
    let store = store::store(state);
    let reg = registry::registry(state);
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
// Snapshot the list under lock, then probe lock-free — each `Repository::open`
// can do significant I/O.
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn workspace_health_scan(
    state: &CorvusState,
    workspace_id: String,
) -> Result<Vec<RepoHealth>, String> {
    // Snapshot the list so we don't hold any locks while probing.
    let entries: Vec<RepoRegistryEntry> = {
        let store = store::store(state);
        let reg = registry::registry(state);
        let ws = store
            .get(&workspace_id)
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
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
