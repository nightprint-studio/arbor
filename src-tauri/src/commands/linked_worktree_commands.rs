// ---------------------------------------------------------------------------
// Tauri commands — Linked Worktrees management.
//
// All mutations save the registry to disk
// (~/.config/arbor/linked_worktrees.toml) and emit
// `arbor://worktree-links-changed` so any open WorktreeLinkManagerModal
// refreshes.
// ---------------------------------------------------------------------------

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
use crate::linked_worktrees::{self, AliasEntry, AliasGroup, WorktreeLink};
use crate::AppState;

fn emit_changed(app: &AppHandle) {
    let _ = app.emit("arbor://worktree-links-changed", serde_json::json!({}));
}

#[tauri::command]
pub fn list_worktree_links(state: State<'_, AppState>) -> Result<Vec<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.list())
}

#[tauri::command]
pub fn get_worktree_link(state: State<'_, AppState>, id: String) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.get(&id).cloned())
}

#[tauri::command]
pub fn get_worktree_link_for_repo(
    state: State<'_, AppState>,
    repo_id: String,
) -> Result<Option<WorktreeLink>, AppError> {
    Ok(state.lock_linked_worktrees()?.find_by_repo(&repo_id).cloned())
}

#[tauri::command]
pub fn create_worktree_link(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    initial_repo_ids: Vec<String>,
) -> Result<WorktreeLink, AppError> {
    let link = {
        let mut reg = state.lock_linked_worktrees()?;
        let l = reg.create(name, initial_repo_ids)?;
        linked_worktrees::save(&reg)?;
        l
    };
    emit_changed(&app);
    Ok(link)
}

#[tauri::command]
pub fn delete_worktree_link(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.delete(&id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn rename_worktree_link(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.rename(&id, name)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn add_worktree_link_member(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.add_member(&link_id, &repo_id)?;
        linked_worktrees::save(&reg)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "link_id": &link_id, "repo_id": &repo_id });
        let _ = host.fire_hook("on_worktree_link_member_added", &ctx.to_string());
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn remove_worktree_link_member(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    repo_id: String,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.remove_member(&link_id, &repo_id)?;
        linked_worktrees::save(&reg)?;
    }
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "link_id": &link_id, "repo_id": &repo_id });
        let _ = host.fire_hook("on_worktree_link_member_removed", &ctx.to_string());
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_worktree_link_sync_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.set_sync_enabled(&link_id, enabled)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_worktree_link_member_sync_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    repo_id: String,
    enabled: bool,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.set_member_sync_enabled(&link_id, &repo_id, enabled)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasGroupInput {
    pub members: Vec<AliasEntry>,
}

#[tauri::command]
pub fn add_alias_group(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    members: Vec<AliasEntry>,
) -> Result<AliasGroup, AppError> {
    let group = {
        let mut reg = state.lock_linked_worktrees()?;
        let g = reg.add_alias_group(&link_id, members)?;
        linked_worktrees::save(&reg)?;
        g
    };
    emit_changed(&app);
    Ok(group)
}

#[tauri::command]
pub fn update_alias_group(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    group_id: String,
    members: Vec<AliasEntry>,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.update_alias_group(&link_id, &group_id, members)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn remove_alias_group(
    app: AppHandle,
    state: State<'_, AppState>,
    link_id: String,
    group_id: String,
) -> Result<(), AppError> {
    {
        let mut reg = state.lock_linked_worktrees()?;
        reg.remove_alias_group(&link_id, &group_id)?;
        linked_worktrees::save(&reg)?;
    }
    emit_changed(&app);
    Ok(())
}
