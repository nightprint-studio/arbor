//! `workspace` domain — mutation handlers, OOP twins in the headless corvus-be
//! backend.
//!
//! Each handler is the byte-for-byte port of the Tauri-shell `workspace`
//! platform handler, re-homed onto [`CorvusState`] and `#[arbor_rpc::handler]`.
//! The workspace store / repo registry / snapshot I/O live in the reusable
//! [`crate::workspace`] modules; handlers delegate to those (reload-on-access
//! guards + `mutate` write seams) so behavior — locks held, store saves,
//! emitted events, fired hooks, errors — is identical to the shell.
//!
//! Because the plugin host is co-located in corvus-be, the fire-and-forget
//! workspace hooks the shell moved into its `post_hooks` table fire **inline**
//! again here, right after the mutation.

use corvus_core::prelude::{hooks, CorvusState};
use serde_json::json;

use crate::workspace::{
    registry, snapshot, store, ExportedBundle, ExportedRepo, ImportBundleResult,
    ImportGroupWorkspaceCommit, RepoRegistrationResult, RepoRegistryEntry, WorkspaceDef,
    WorkspaceGroup, WorkspaceGroupPatch, WorkspacePatch, SCRATCH_ID,
};

// ---------------------------------------------------------------------------
// Mutation commands — workspaces.
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn create_workspace(
    state: &CorvusState,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
) -> Result<WorkspaceDef, String> {
    let ws = store::mutate(state, |store| {
        let ws = store.create(name, color_idx, repo_ids, group_id);
        Ok(ws)
    })?;
    state.fire_hook(hooks::WORKSPACE_CREATED, crate::workspace::workspace_payload(&ws));
    crate::workspace::emit_registry_changed(state);
    Ok(ws)
}

#[arbor_rpc::handler]
fn update_workspace(
    state: &CorvusState,
    workspace_id: String,
    patch: WorkspacePatch,
) -> Result<WorkspaceDef, String> {
    let ws = store::mutate(state, |store| {
        {
            let ws = store
                .get_mut(&workspace_id)
                .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
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
        store
            .get(&workspace_id)
            .cloned()
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))
    })?;
    state.fire_hook(hooks::WORKSPACE_UPDATED, crate::workspace::workspace_payload(&ws));
    crate::workspace::emit_registry_changed(state);
    Ok(ws)
}

#[arbor_rpc::handler]
fn reorder_workspaces(state: &CorvusState, ordered_ids: Vec<String>) -> Result<(), String> {
    store::mutate(state, |store| {
        store.set_order(&ordered_ids);
        Ok(())
    })?;
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn add_repo_to_workspace(
    state: &CorvusState,
    workspace_id: String,
    repo_id: String,
) -> Result<(), String> {
    store::mutate(state, |store| {
        store.add_repo(&workspace_id, &repo_id)?;
        Ok(())
    })?;
    state.fire_hook(
        hooks::WORKSPACE_REPO_ADDED,
        json!({ "workspace_id": &workspace_id, "repo_id": &repo_id }),
    );
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn move_repo_between_workspaces(
    state: &CorvusState,
    from_workspace_id: String,
    to_workspace_id: String,
    repo_id: String,
) -> Result<(), String> {
    store::mutate(state, |store| {
        store.remove_repo(&from_workspace_id, &repo_id)?;
        store.add_repo(&to_workspace_id, &repo_id)?;
        Ok(())
    })?;
    state.fire_hook(
        hooks::WORKSPACE_REPO_REMOVED,
        json!({ "workspace_id": &from_workspace_id, "repo_id": &repo_id }),
    );
    state.fire_hook(
        hooks::WORKSPACE_REPO_ADDED,
        json!({ "workspace_id": &to_workspace_id, "repo_id": &repo_id }),
    );
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

// ---------------------------------------------------------------------------
// Repo registry — registration + editing.
// ---------------------------------------------------------------------------

/// Upsert a repo path into the registry AND auto-add it to the active
/// workspace if it isn't already a member of it.
#[arbor_rpc::handler]
fn register_repo_path(
    state: &CorvusState,
    path: String,
    remote_url: Option<String>,
    display_name: Option<String>,
) -> Result<RepoRegistrationResult, String> {
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
    let remote_url = remote_url.or_else(|| crate::repo::origin_url(&path));
    let (id, existed) = registry::mutate(state, |reg| {
        let existed = reg.find_by_path(&path).is_some();
        let id = reg.upsert_by_path(&path, remote_url, &fallback_name);
        Ok((id, existed))
    })?;
    // Auto-add to active workspace if missing.
    let added_to_ws = store::mutate(state, |store| {
        let active = store
            .active_workspace_id
            .clone()
            .unwrap_or_else(|| SCRATCH_ID.to_string());
        let ws = store
            .get_mut(&active)
            .ok_or_else(|| format!("active workspace not found: {active}"))?;
        if ws.repo_ids.iter().any(|i| i == &id) {
            Ok(false)
        } else {
            ws.repo_ids.push(id.clone());
            Ok(true)
        }
    })?;
    crate::workspace::emit_registry_changed(state);
    Ok(RepoRegistrationResult { id, existed, added_to_ws })
}

/// Create a "pending" registry entry for a repo that's declared (name +
/// optional remote URL) but not yet on disk — used by the non-blocking
/// workspace import.  Returns the new id.
#[arbor_rpc::handler]
fn register_pending_repo(
    state: &CorvusState,
    name: String,
    remote_url: Option<String>,
) -> Result<String, String> {
    let id = registry::mutate(state, |reg| {
        let id = reg.insert_pending(remote_url, &name);
        Ok(id)
    })?;
    crate::workspace::emit_registry_changed(state);
    Ok(id)
}

#[arbor_rpc::handler]
fn update_registry_repo(
    state: &CorvusState,
    repo_id: String,
    display_name: Option<String>,
    remote_url: Option<Option<String>>,
    path: Option<String>,
) -> Result<RepoRegistryEntry, String> {
    let entry = registry::mutate(state, |reg| {
        if let Some(name) = display_name {
            reg.set_display_name(&repo_id, name)?;
        }
        if let Some(url) = remote_url {
            reg.set_remote_url(&repo_id, url)?;
        }
        if let Some(p) = path {
            reg.set_path(&repo_id, p)?;
        }
        reg.get(&repo_id)
            .cloned()
            .ok_or_else(|| format!("repo not found: {repo_id}"))
    })?;
    crate::workspace::emit_registry_changed(state);
    Ok(entry)
}

#[arbor_rpc::handler]
fn delete_registry_repo(state: &CorvusState, repo_id: String) -> Result<(), String> {
    // Capture path/name BEFORE removal so the hook payload is meaningful.
    let path_name = {
        let reg = registry::registry(state);
        reg.get(&repo_id).map(|e| (e.path.clone(), e.display_name.clone()))
    };
    store::mutate(state, |store| {
        store.purge_repo_everywhere(&repo_id);
        Ok(())
    })?;
    registry::mutate(state, |reg| {
        reg.remove(&repo_id);
        Ok(())
    })?;
    if let Some((path, name)) = path_name {
        crate::workspace::forget_recent_repo(state, &path);
        state.fire_hook(
            hooks::REPO_DEREGISTERED,
            json!({
                "repo_id": repo_id,
                "path":    path,
                "name":    name,
                "reason":  "registry_delete",
            }),
        );
    }
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mutation commands — groups (no FE event, no hook → leaf-clean)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn create_workspace_group(
    state: &CorvusState,
    name: String,
    color_idx: u8,
) -> Result<WorkspaceGroup, String> {
    store::mutate(state, |store| {
        let g = store.create_group(name, color_idx);
        Ok(g)
    })
}

#[arbor_rpc::handler]
fn update_workspace_group(
    state: &CorvusState,
    group_id: String,
    patch: WorkspaceGroupPatch,
) -> Result<WorkspaceGroup, String> {
    store::mutate(state, |store| {
        {
            let g = store
                .get_group_mut(&group_id)
                .ok_or_else(|| format!("group not found: {group_id}"))?;
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
        store
            .get_group(&group_id)
            .cloned()
            .ok_or_else(|| format!("group not found: {group_id}"))
    })
}

#[arbor_rpc::handler]
fn delete_workspace_group(state: &CorvusState, group_id: String) -> Result<(), String> {
    store::mutate(state, |store| {
        store.remove_group(&group_id)?;
        Ok(())
    })
}

#[arbor_rpc::handler]
fn reorder_workspace_groups(
    state: &CorvusState,
    ordered_ids: Vec<String>,
) -> Result<(), String> {
    store::mutate(state, |store| {
        store.set_group_order(&ordered_ids);
        Ok(())
    })
}

#[arbor_rpc::handler]
fn set_workspace_group(
    state: &CorvusState,
    workspace_id: String,
    group_id: Option<String>,
) -> Result<(), String> {
    store::mutate(state, |store| {
        store.set_workspace_group(&workspace_id, group_id)?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Workspace mutations with pre-mutation-state hooks (fired inline).
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn delete_workspace(state: &CorvusState, workspace_id: String) -> Result<(), String> {
    if workspace_id == SCRATCH_ID {
        return Err("cannot delete the Scratch workspace".to_string());
    }
    // Capture the payload and the member list before mutating: the members are
    // the GC candidates once the workspace is gone.
    let (deleted_payload, member_ids) = {
        let store = store::store(state);
        let payload = store.get(&workspace_id).map(crate::workspace::workspace_payload);
        let members = store.get(&workspace_id).map(|w| w.repo_ids.clone()).unwrap_or_default();
        (payload, members)
    };
    store::mutate(state, |store| {
        store.remove(&workspace_id)?;
        Ok(())
    })?;
    // Best-effort: delete the snapshot file too.
    let _ = snapshot::delete(state, &workspace_id);
    if let Some(payload) = deleted_payload {
        state.fire_hook(hooks::WORKSPACE_DELETED, payload);
    }
    // Forget every member that's no longer referenced by another workspace, so
    // Arbor stops proposing it as "use existing" on a later import.
    for repo_id in member_ids {
        let _ = crate::workspace::forget_repo_if_orphaned(state, &repo_id, "workspace_deleted");
    }
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

#[arbor_rpc::handler]
fn set_active_workspace(state: &CorvusState, workspace_id: String) -> Result<WorkspaceDef, String> {
    let (from_id, ws) = store::mutate(state, |store| {
        let target = store
            .get(&workspace_id)
            .cloned()
            .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
        let from = store.active_workspace_id.clone();
        store.active_workspace_id = Some(workspace_id.clone());
        Ok((from, target))
    })?;
    let mut payload = crate::workspace::workspace_payload(&ws);
    if let Some(from) = from_id {
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("from_id".into(), serde_json::Value::String(from));
        }
    }
    state.emit("arbor://workspace-switched", payload.clone());
    state.fire_hook(hooks::WORKSPACE_SWITCHED, payload);
    Ok(ws)
}

#[arbor_rpc::handler]
fn remove_repo_from_workspace(
    state: &CorvusState,
    workspace_id: String,
    repo_id: String,
) -> Result<(), String> {
    store::mutate(state, |store| {
        store.remove_repo(&workspace_id, &repo_id)?;
        Ok(())
    })?;
    state.fire_hook(
        hooks::WORKSPACE_REPO_REMOVED,
        json!({
            "workspace_id": workspace_id,
            "repo_id":      repo_id,
        }),
    );
    // If that was the repo's last workspace, forget it entirely.
    let _ = crate::workspace::forget_repo_if_orphaned(state, &repo_id, "removed_from_last_workspace");
    crate::workspace::emit_registry_changed(state);
    Ok(())
}

// ---------------------------------------------------------------------------
// Import commits (variable-count hooks → inline).
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
fn import_workspace_commit(
    state: &CorvusState,
    name: String,
    color_idx: u8,
    repo_ids: Vec<String>,
    group_id: Option<String>,
    merge_into: Option<String>,
) -> Result<WorkspaceDef, String> {
    let (ws, merged) = store::mutate(state, |store| {
        let result = match merge_into.filter(|id| store.get(id).is_some()) {
            Some(id) => {
                store.merge_repos_into(&id, &repo_ids)?;
                let ws = store
                    .get(&id)
                    .cloned()
                    .ok_or_else(|| format!("workspace not found: {id}"))?;
                (ws, true)
            }
            None => (store.create(name, color_idx, repo_ids, group_id), false),
        };
        Ok(result)
    })?;
    state.fire_hook(
        if merged { hooks::WORKSPACE_UPDATED } else { hooks::WORKSPACE_CREATED },
        crate::workspace::workspace_payload(&ws),
    );
    Ok(ws)
}

#[arbor_rpc::handler]
fn import_workspace_group_commit(
    state: &CorvusState,
    name: String,
    color_idx: u8,
    existing_group_id: Option<String>,
    workspaces: Vec<ImportGroupWorkspaceCommit>,
) -> Result<(), String> {
    // (workspace, was_merged) for each member, to fire the right hook after.
    let touched: Vec<(WorkspaceDef, bool)> = store::mutate(state, |store| {
        let (group_id, merged_group) =
            match existing_group_id.filter(|id| store.get_group(id).is_some()) {
                Some(id) => (id, true),
                None => (store.create_group(name, color_idx).id, false),
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
                    if let Some(ws) = store.get(&id).cloned() {
                        touched.push((ws, true));
                    }
                }
                None => {
                    let ws = store.create(w.name, w.color_idx, w.repo_ids, Some(group_id.clone()));
                    touched.push((ws, false));
                }
            }
        }
        Ok(touched)
    })?;
    for (ws, merged) in &touched {
        state.fire_hook(
            if *merged { hooks::WORKSPACE_UPDATED } else { hooks::WORKSPACE_CREATED },
            crate::workspace::workspace_payload(ws),
        );
    }
    Ok(())
}

#[arbor_rpc::handler]
fn import_bundle_commit(
    state: &CorvusState,
    payload: ExportedBundle,
) -> Result<ImportBundleResult, String> {
    // Dedup key: prefer the remote URL, fall back to the display name.
    fn dedup_key(r: &ExportedRepo) -> String {
        match r.remote_url.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
            Some(u) => format!("url:{}", u.to_lowercase()),
            None => format!("name:{}", r.name.trim().to_lowercase()),
        }
    }

    let mut result = ImportBundleResult::default();

    // Pass 1 — resolve every distinct repo across the bundle to a registry id.
    let mut key_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    registry::mutate(state, |reg| {
        let mut all: Vec<&ExportedRepo> = Vec::new();
        for g in &payload.groups {
            for w in &g.workspaces {
                all.extend(w.repos.iter());
            }
        }
        for w in &payload.workspaces {
            all.extend(w.repos.iter());
        }
        for r in all {
            let key = dedup_key(r);
            if key_to_id.contains_key(&key) {
                continue;
            }
            let existing = r
                .remote_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .and_then(|u| reg.find_by_remote_url(u))
                .map(|e| e.id.clone());
            let id = match existing {
                Some(id) => {
                    result.repos_linked += 1;
                    id
                }
                None => {
                    result.repos_pending += 1;
                    reg.insert_pending(r.remote_url.clone(), &r.name)
                }
            };
            key_to_id.insert(key, id);
        }
        Ok(())
    })?;

    let ids_of = |repos: &[ExportedRepo]| -> Vec<String> {
        let mut out = Vec::new();
        for r in repos {
            if let Some(id) = key_to_id.get(&dedup_key(r)) {
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
        out
    };

    // Pass 2 — rebuild groups + workspaces (idempotent merge by name).
    let touched: Vec<(WorkspaceDef, bool)> = store::mutate(state, |store| {
        let mut touched: Vec<(WorkspaceDef, bool)> = Vec::new();
        for g in &payload.groups {
            let existing_gid = store
                .groups
                .iter()
                .find(|x| x.name.eq_ignore_ascii_case(g.name.trim()))
                .map(|x| x.id.clone());
            let (gid, merged_group) = match existing_gid {
                Some(id) => {
                    result.groups_merged += 1;
                    (id, true)
                }
                None => {
                    result.groups_created += 1;
                    (store.create_group(g.name.clone(), g.color_idx).id, false)
                }
            };
            for w in &g.workspaces {
                let ids = ids_of(&w.repos);
                let target = if merged_group {
                    store.find_by_name_in_group(&w.name, Some(&gid)).map(|x| x.id.clone())
                } else {
                    None
                };
                match target {
                    Some(id) => {
                        store.merge_repos_into(&id, &ids)?;
                        if let Some(ws) = store.get(&id).cloned() {
                            touched.push((ws, true));
                        }
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
                    if let Some(ws) = store.get(&id).cloned() {
                        touched.push((ws, true));
                    }
                    result.workspaces_merged += 1;
                }
                None => {
                    let ws = store.create(w.name.clone(), w.color_idx, ids, None);
                    touched.push((ws, false));
                    result.workspaces_created += 1;
                }
            }
        }
        Ok(touched)
    })?;
    for (ws, merged) in &touched {
        state.fire_hook(
            if *merged { hooks::WORKSPACE_UPDATED } else { hooks::WORKSPACE_CREATED },
            crate::workspace::workspace_payload(ws),
        );
    }
    Ok(result)
}
