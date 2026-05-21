use tauri::State;

use crate::error::AppError;
use crate::git::stash::{StashApplyResult, StashBlockingContent, StashEntry, StashRef};
use crate::AppState;

#[tauri::command]
pub fn list_stashes(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<StashEntry>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::list_stashes(repo.inner_mut())
}

/// Returns the same `Vec<StashRef>` that `get_graph` embeds in `GraphData.stashes`.
///
/// Used after a stash op (save / drop / pop / apply) to repaint just the
/// stash markers without re-running the gitk lane assignment over the
/// whole history — the graph topology is unchanged by stash operations.
#[tauri::command]
pub fn list_graph_stash_refs(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<StashRef>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::collect_stash_refs(repo.inner_mut())
}

#[tauri::command]
pub fn stash_save(
    state: State<'_, AppState>,
    tab_id: String,
    message: Option<String>,
    include_untracked: bool,
) -> Result<StashEntry, AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };
    let entry = crate::git::stash::stash_save(&workdir, message.as_deref(), include_untracked)?;
    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":            &tab_id,
            "index":             entry.index,
            "message":           &entry.message,
            "include_untracked": include_untracked,
        });
        let _ = host.fire_hook("on_stash_push", &ctx.to_string());
    }
    Ok(entry)
}

#[tauri::command]
pub fn stash_apply(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
) -> Result<StashApplyResult, AppError> {
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        crate::git::stash::stash_apply(repo.inner_mut(), index)?
    };
    // Only fire hook when clean apply (no conflicts)
    if !result.has_conflicts {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "index": index, "drop": false });
            let _ = host.fire_hook("on_stash_pop", &ctx.to_string());
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn stash_pop(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
) -> Result<StashApplyResult, AppError> {
    let result = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        crate::git::stash::stash_pop(repo.inner_mut(), index)?
    };
    // Only fire hook when clean pop (no conflicts) — if conflicted, stash is still present
    if !result.has_conflicts {
        if let Ok(host) = state.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "index": index, "drop": true });
            let _ = host.fire_hook("on_stash_pop", &ctx.to_string());
        }
    }
    Ok(result)
}

#[tauri::command]
pub fn force_stash_apply(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
    files_to_delete: Vec<String>,
    files_to_keep: Vec<String>,
    drop_on_success: bool,
) -> Result<StashApplyResult, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::force_stash_apply(repo.inner_mut(), index, &files_to_delete, &files_to_keep, drop_on_success)
}

#[tauri::command]
pub fn abort_stash_apply(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    // Hold the lock across the snapshot+abort so a concurrent operation cannot
    // race with us and overwrite the snapshot's implicit refs.
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    let workdir = repo.inner()
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
        .to_path_buf();
    crate::git::stash::abort_stash_apply_with_snapshot(&workdir, Some(repo.inner()))
}

/// Write a file into the repository working directory.
/// Used to persist a custom-merged blocking file before `force_stash_apply`.
///
/// `encoding` is the label returned by `get_stash_file_content` — round-trip
/// it back here so legacy windows-1252 / Latin-1 files don't get silently
/// rewritten as UTF-8.
#[tauri::command]
pub fn write_workdir_file(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
    content: String,
    encoding: Option<String>,
) -> Result<(), AppError> {
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };
    let abs = workdir.join(&path);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Other(format!("failed to create parent dirs for {path}: {e}")))?;
    }
    let bytes = crate::git::encoding::encode_for_disk(&content, encoding.as_deref());
    std::fs::write(&abs, &bytes)
        .map_err(|e| AppError::Other(format!("failed to write {path}: {e}")))?;
    Ok(())
}

#[tauri::command]
pub fn get_stash_file_content(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
    path: String,
    encoding_override: Option<String>,
) -> Result<StashBlockingContent, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::stash::get_stash_file_content(
        repo.inner(), index, &path, encoding_override.as_deref(),
    )
}

#[tauri::command]
pub fn stash_drop(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
) -> Result<(), AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::stash_drop(repo.inner_mut(), index)
}

#[tauri::command]
pub fn stash_rename(
    state: State<'_, AppState>,
    tab_id: String,
    index: usize,
    new_message: String,
) -> Result<StashEntry, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get_mut(&tab_id)?;
    crate::git::stash::stash_rename(repo.inner_mut(), index, &new_message)
}
