//! `index` domain — project-wide `.ron`/`.json`/`.toml`/… scanners powering
//! the Studio sidebar (file walk, cross-reference graph, reverse usages,
//! broken-reference detection).
//!
//! Each handler is the body the matching `#[tauri::command] async fn` ran
//! inside `spawn_blocking`, now a plain sync function self-registered under
//! `program = "studio"`. The per-handler `spawn_blocking` is gone: the generic
//! `rpc` command already dispatches every handler inside one central
//! `spawn_blocking` (see `crate::commands::rpc_commands`), so the heavy walk
//! still runs off the Tauri runtime workers — behavior is identical.
//!
//! `studio_refresh_index` is **not** here: it captures an `AppHandle`, emits
//! `arbor://studio-index-*` progress events, and fire-and-forget spawns the
//! walk on the blocking pool. It stays inline as a keep-shell Tauri command.

use crate::error::AppError;
use crate::ipc::studio;
use crate::studio::{
    find_usages_for, index, scan_broken_refs_for, scan_cross_refs_for, scan_repo, BrokenRef,
    CrossRefDef, StudioFileEntry, StudioFileKind, UsageMatch,
};
use crate::AppState;

/// Scan the active tab's repository for indexable data files. `kinds`
/// filters the result: empty vec means "all supported kinds".
#[studio::handler(program = "studio")]
fn studio_scan_repo(
    state: &AppState,
    tab_id: String,
    kinds: Vec<StudioFileKind>,
) -> Result<Vec<StudioFileEntry>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    scan_repo(&repo_path, &kinds)
}

/// Project-wide cross-reference scan. Returns every `id: "…"` /
/// `name: "…"` definition across the active repo's files (RON / JSON
/// — `kinds` defaults to `[Ron]` when empty for back-compat). The
/// frontend folds the list into a `Map<id, Vec<def>>` per kind so a
/// single id duplicated across two files surfaces both targets in the
/// Ctrl+click picker.
#[studio::handler(program = "studio")]
fn studio_scan_cross_refs(
    state: &AppState,
    tab_id: String,
    kinds: Option<Vec<StudioFileKind>>,
) -> Result<Vec<CrossRefDef>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state
        .lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    // Empty list = back-compat RON-only; explicit list = filter.
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    if use_index {
        let idx = index::load(&repo_path);
        if !idx.files.is_empty() {
            return Ok(index::aggregate_cross_refs_for(&idx, &kinds));
        }
    }
    scan_cross_refs_for(&repo_path, &kinds)
}

/// Reverse navigation: given a top-level `id`/`name` value, find every
/// reference field across the project pointing at it. Drives the
/// "Used by N files" panel on definition nodes.
#[studio::handler(program = "studio")]
fn studio_find_usages(
    state: &AppState,
    tab_id: String,
    target: String,
    kinds: Option<Vec<StudioFileKind>>,
) -> Result<Vec<UsageMatch>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state
        .lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    if use_index {
        let idx = index::load(&repo_path);
        if !idx.files.is_empty() {
            return Ok(index::aggregate_usages_for(&idx, &target, &kinds));
        }
    }
    find_usages_for(&repo_path, &target, &kinds)
}

/// Project-wide broken-reference scan. Walks every `.ron` file in the
/// repo (skipping excludes), gathers every `id`/`name` definition,
/// and emits every reference whose value doesn't appear in that
/// def set — useful for catching renamed/deleted entities before
/// they ship as silently-dead pointers at runtime. Result is sorted
/// by orphan value so the same broken target groups visually.
#[studio::handler(program = "studio")]
fn studio_scan_broken_refs(
    state: &AppState,
    tab_id: String,
    kinds: Option<Vec<StudioFileKind>>,
) -> Result<Vec<BrokenRef>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state
        .lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    if use_index {
        let idx = index::load(&repo_path);
        if !idx.files.is_empty() {
            return Ok(index::aggregate_broken_refs_for(&idx, &kinds));
        }
    }
    scan_broken_refs_for(&repo_path, &kinds)
}
