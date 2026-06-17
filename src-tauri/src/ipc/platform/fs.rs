//! `fs` domain — pure filesystem operations routed through the platform broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline; the FS
//! logic itself lives in the reusable, Tauri-free [`arbor_fs`] crate
//! (read/write/copy/move/delete/list/search/trash/zip, roots, sizes, path
//! expansion). Handlers map `FsError` → [`AppError`] and otherwise delegate
//! straight to it.
//!
//! ## No more per-command `spawn_blocking`
//!
//! The old async commands wrapped each blocking call in
//! `tokio::task::spawn_blocking`. The generic `rpc` command now runs *every*
//! platform/corvus handler on the blocking pool already (see
//! `crate::commands::rpc_commands::rpc`), so handlers are plain sync functions
//! that call the blocking `arbor-fs` functions directly — off the main thread,
//! identical behavior, minus the wrapper.
//!
//! ## What stays in the shell (NOT here)
//!
//! The OS / Tauri shell-integration that is *not* pure FS I/O remains inline in
//! [`crate::commands::fs_commands`]: open-with-default, reveal-in-file-manager,
//! open-terminal, the native Properties dialog, set-wallpaper, native icons, and
//! the per-window watcher. The progress-emitting `fs_copy` / `fs_move` /
//! `fs_duplicate` / `fs_cancel_op` ops also stay there for the later emit/seam
//! pass — they hold an `AppHandle` and stream `arbor://fs-op-progress`.
//!
//! These handlers never touched `AppState`, but the handler macro requires a
//! context first arg, so they take `_state: &AppState` and ignore it.
//!
//! No hooks fire in this domain.

use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;
use arbor_fs::prelude::{
    mutate, pathexpand, read, roots, size, trash, zip, DirSize, FsEntry, FsRoot, OverviewStats,
    RenamePair, TrashEntry,
};

// ---------------------------------------------------------------------------
// Mutating filesystem operations
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn fs_create_dir(_state: &AppState, path: String) -> Result<(), AppError> {
    Ok(mutate::create_dir(&path)?)
}

#[platform::handler(program = "platform")]
fn fs_create_file(_state: &AppState, path: String) -> Result<(), AppError> {
    Ok(mutate::create_file(&path)?)
}

#[platform::handler(program = "platform")]
fn fs_rename(_state: &AppState, old_path: String, new_path: String) -> Result<(), AppError> {
    Ok(mutate::rename(&old_path, &new_path)?)
}

#[platform::handler(program = "platform")]
fn fs_rename_many(_state: &AppState, pairs: Vec<RenamePair>) -> Result<Vec<String>, AppError> {
    Ok(mutate::rename_many(&pairs)?)
}

#[platform::handler(program = "platform")]
fn fs_write_text_file(_state: &AppState, path: String, content: String) -> Result<(), AppError> {
    Ok(mutate::write_text(&path, &content)?)
}

#[platform::handler(program = "platform")]
fn fs_read_text_file(_state: &AppState, path: String) -> Result<String, AppError> {
    Ok(read::read_text(&path)?)
}

#[platform::handler(program = "platform")]
fn fs_delete(_state: &AppState, path: String) -> Result<(), AppError> {
    Ok(mutate::delete(&path)?)
}

#[platform::handler(program = "platform")]
fn fs_delete_many(_state: &AppState, paths: Vec<String>) -> Result<(), AppError> {
    Ok(mutate::delete_many(&paths)?)
}

// ---------------------------------------------------------------------------
// Listing + search
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn fs_read_dir(
    _state: &AppState,
    path: String,
    show_hidden: Option<bool>,
) -> Result<Vec<FsEntry>, AppError> {
    Ok(read::read_dir(&path, show_hidden.unwrap_or(false))?)
}

#[platform::handler(program = "platform")]
fn fs_search(
    _state: &AppState,
    root: String,
    query: String,
    show_hidden: Option<bool>,
    limit: Option<usize>,
) -> Result<Vec<FsEntry>, AppError> {
    let show_hidden = show_hidden.unwrap_or(false);
    let cap = limit.unwrap_or(5000);
    Ok(read::search(&root, &query, show_hidden, cap)?)
}

// ---------------------------------------------------------------------------
// Trash — recoverable delete + Recycle Bin view
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn fs_trash(_state: &AppState, paths: Vec<String>) -> Result<(), AppError> {
    Ok(trash::trash(&paths)?)
}

#[platform::handler(program = "platform")]
fn fs_untrash(_state: &AppState, paths: Vec<String>) -> Result<(), AppError> {
    Ok(trash::untrash(&paths)?)
}

#[platform::handler(program = "platform")]
fn fs_trash_list(_state: &AppState) -> Result<Vec<TrashEntry>, AppError> {
    Ok(trash::trash_list()?)
}

#[platform::handler(program = "platform")]
fn fs_trash_restore(_state: &AppState, ids: Vec<String>) -> Result<(), AppError> {
    Ok(trash::trash_restore(&ids)?)
}

#[platform::handler(program = "platform")]
fn fs_trash_purge(_state: &AppState, ids: Vec<String>) -> Result<(), AppError> {
    Ok(trash::trash_purge(&ids)?)
}

#[platform::handler(program = "platform")]
fn fs_trash_empty(_state: &AppState) -> Result<(), AppError> {
    Ok(trash::trash_empty()?)
}

// ---------------------------------------------------------------------------
// Compress / extract — ZIP archives
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn fs_zip(
    _state: &AppState,
    sources: Vec<String>,
    dest_dir: String,
    archive_name: String,
) -> Result<String, AppError> {
    Ok(zip::zip(&sources, &dest_dir, &archive_name)?)
}

#[platform::handler(program = "platform")]
fn fs_unzip(
    _state: &AppState,
    archive: String,
    dest_dir: Option<String>,
) -> Result<String, AppError> {
    Ok(zip::unzip(&archive, dest_dir)?)
}

// ---------------------------------------------------------------------------
// Recursive sizes + quick-access roots + WSL + Overview stats
// ---------------------------------------------------------------------------

#[platform::handler(program = "platform")]
fn fs_dir_size(_state: &AppState, path: String) -> Result<DirSize, AppError> {
    Ok(size::dir_size(&path))
}

#[platform::handler(program = "platform")]
fn fs_paths_size(_state: &AppState, paths: Vec<String>) -> Result<DirSize, AppError> {
    Ok(size::paths_size(&paths))
}

/// Return filesystem quick-access roots (common user dirs + drives / `/`).
#[platform::handler(program = "platform")]
fn list_fs_roots(_state: &AppState) -> Result<Vec<FsRoot>, AppError> {
    Ok(roots::list_roots())
}

/// List installed WSL distributions as navigable roots.
#[platform::handler(program = "platform")]
fn list_wsl_distros(_state: &AppState) -> Result<Vec<FsRoot>, AppError> {
    Ok(roots::list_wsl_distros())
}

/// Real Overview dashboard stats: capacity / free space per drive.
#[platform::handler(program = "platform")]
fn fs_overview_stats(_state: &AppState) -> Result<OverviewStats, AppError> {
    Ok(roots::overview_stats())
}

/// Expand environment-variable references and a leading `~` in a user-typed
/// path (`%appdata%`, `$HOME`, `~/code`, …). Unknown variables are left intact.
#[platform::handler(program = "platform")]
fn fs_expand_path(_state: &AppState, path: String) -> Result<String, AppError> {
    Ok(pathexpand::expand_path(&path))
}
