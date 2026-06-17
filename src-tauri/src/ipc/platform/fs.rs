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
//! ## Copy / move / duplicate / cancel — progress through the event sink
//!
//! These four ops stream a throttled `arbor://fs-op-progress` event and own a
//! process-global op-id → [`CancelToken`] registry. A handler reached through
//! the generic `rpc` command holds only `&AppState` (no `AppHandle`), so the
//! progress sink forwards through the **event sink**
//! (`Arc<dyn EventSink>` — [`AppState::event_sink`]) instead of an `AppHandle`.
//! The cancel-token registry is a `OnceLock` static, so it stays reachable from
//! the handler unchanged. Behavior is byte-identical: same topic, same payload
//! shape, same throttling, same registry effects — only the egress handle
//! changed. The old per-command `spawn_blocking` is gone too (see below): the op
//! body runs synchronously on the rpc blocking pool.
//!
//! ## What stays in the shell (NOT here)
//!
//! The OS / Tauri shell-integration that is *not* pure FS I/O remains inline in
//! [`crate::commands::fs_commands`]: open-with-default, reveal-in-file-manager,
//! open-terminal, the native Properties dialog, set-wallpaper, native icons, and
//! the per-window watcher.
//!
//! The pure-FS handlers never touched `AppState`, but the handler macro requires
//! a context first arg, so they take `_state: &AppState` and ignore it. The
//! copy/move/duplicate handlers use it for `event_sink()`.
//!
//! No hooks fire in this domain.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use arbor_ipc::prelude::EventSink;

use crate::error::AppError;
use crate::ipc::platform;
use crate::AppState;
use arbor_fs::prelude::{
    copy, mutate, pathexpand, read, roots, size, trash, zip, CancelToken, DirSize, FsEntry, FsRoot,
    NoopSink, OverviewStats, ProgressSink, RenamePair, TrashEntry,
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

// ---------------------------------------------------------------------------
// Copy / move / duplicate — progress + cooperative cancellation
// ---------------------------------------------------------------------------
// A per-`op_id` cancel token lets the UI abort a running op; a throttled
// `arbor://fs-op-progress` event drives the explorer's progress bar. The pure
// copy/move/duplicate live in `arbor-fs`; here we own the registry and the sink.

/// Process-wide registry of cancel tokens keyed by op_id.
fn cancel_registry() -> &'static Mutex<HashMap<String, CancelToken>> {
    static REG: OnceLock<Mutex<HashMap<String, CancelToken>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_op(op_id: &str) -> CancelToken {
    let token = CancelToken::new();
    if let Ok(mut m) = cancel_registry().lock() {
        m.insert(op_id.to_string(), token.clone());
    }
    token
}

fn unregister_op(op_id: &str) {
    if let Ok(mut m) = cancel_registry().lock() {
        m.remove(op_id);
    }
}

/// Request cancellation of a running file operation. No-op for unknown ids
/// (the op may have already finished). Cooperative: the op stops at the next
/// file boundary. Signals the process-global registry only — emits nothing.
#[platform::handler(program = "platform")]
fn fs_cancel_op(_state: &AppState, op_id: String) -> Result<(), AppError> {
    if let Ok(m) = cancel_registry().lock() {
        if let Some(token) = m.get(&op_id) {
            token.cancel();
        }
    }
    Ok(())
}

/// Progress sink that emits the throttled `arbor://fs-op-progress` event through
/// the backend event sink. The totals arrive in `start`; per-file frames are
/// throttled to ~80ms, while the initial (0%) and final (100%) frames are forced
/// so the bar appears and completes. The payload (`op_id`/`kind`/`done_files`/
/// `total_files`/`done_bytes`/`total_bytes`/`current`) is byte-identical to the
/// old `AppHandle`-based emit.
struct EmitSink {
    sink:        Arc<dyn EventSink>,
    op_id:       String,
    kind:        &'static str,
    total_files: u64,
    total_bytes: u64,
    last_emit:   Instant,
}

impl EmitSink {
    fn emit(&mut self, done_files: u64, done_bytes: u64, current: &str, force: bool) {
        if !force && self.last_emit.elapsed().as_millis() < 80 {
            return;
        }
        self.last_emit = Instant::now();
        self.sink.emit("arbor://fs-op-progress", serde_json::json!({
            "op_id":       self.op_id,
            "kind":        self.kind,
            "done_files":  done_files,
            "total_files": self.total_files,
            "done_bytes":  done_bytes,
            "total_bytes": self.total_bytes,
            "current":     current,
        }));
    }
}

impl ProgressSink for EmitSink {
    fn start(&mut self, total_files: u64, total_bytes: u64) {
        self.total_files = total_files;
        self.total_bytes = total_bytes;
        self.emit(0, 0, "", true);
    }
    fn file_done(&mut self, done_files: u64, done_bytes: u64, current: &str) {
        self.emit(done_files, done_bytes, current, false);
    }
    fn finish(&mut self, done_files: u64, done_bytes: u64) {
        self.emit(done_files, done_bytes, "", true);
    }
}

/// Run an op closure with the right progress sink: an [`EmitSink`] when the
/// caller supplied an `op_id` (live progress), or a no-op sink otherwise.
fn run_op<F>(
    sink: &Arc<dyn EventSink>,
    op_id: &Option<String>,
    kind: &'static str,
    f: F,
) -> Result<Vec<String>, AppError>
where
    F: FnOnce(&mut dyn ProgressSink) -> arbor_fs::prelude::Result<Vec<String>>,
{
    let res = match op_id {
        Some(id) => {
            let mut emit = EmitSink {
                sink: Arc::clone(sink),
                op_id: id.clone(),
                kind,
                total_files: 0,
                total_bytes: 0,
                last_emit: Instant::now(),
            };
            f(&mut emit)
        }
        None => f(&mut NoopSink),
    };
    res.map_err(AppError::from)
}

/// Copy each of `sources` into `dest_dir`. With `overwrite = false` (default)
/// name collisions are resolved Explorer-style (" (2)", " (3)", …); with
/// `overwrite = true` each item keeps its name and merges into any existing
/// folder of the same name, replacing colliding files (recursive). Returns the
/// list of created / merged destination paths.
#[platform::handler(program = "platform")]
fn fs_copy(
    state: &AppState,
    sources: Vec<String>,
    dest_dir: String,
    overwrite: Option<bool>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let overwrite = overwrite.unwrap_or(false);
    let cancel = match &op_id {
        Some(id) => register_op(id),
        None => CancelToken::new(),
    };
    let out = run_op(&sink, &op_id, "copy", |sink| {
        copy::copy(&sources, &dest_dir, overwrite, sink, &cancel)
    });
    if let Some(id) = &op_id {
        unregister_op(id);
    }
    out
}

/// Move each of `sources` into `dest_dir` (cut + paste). Falls back to
/// copy-then-delete across volumes where `rename` can't work. With
/// `overwrite = true` an item keeps its name and merges into / replaces an
/// existing same-named entry instead of getting a " (2)" suffix. Returns the
/// list of new paths. Moving into the same directory is a no-op.
#[platform::handler(program = "platform")]
fn fs_move(
    state: &AppState,
    sources: Vec<String>,
    dest_dir: String,
    overwrite: Option<bool>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let overwrite = overwrite.unwrap_or(false);
    let cancel = match &op_id {
        Some(id) => register_op(id),
        None => CancelToken::new(),
    };
    let out = run_op(&sink, &op_id, "move", |sink| {
        copy::move_(&sources, &dest_dir, overwrite, sink, &cancel)
    });
    if let Some(id) = &op_id {
        unregister_op(id);
    }
    out
}

/// Duplicate each of `paths` in place (same parent folder), Explorer-style:
/// `report.pdf` → `report (2).pdf`, a second time → `report (3).pdf`. Returns
/// the created paths. Progress/cancel work exactly like `fs_copy`.
#[platform::handler(program = "platform")]
fn fs_duplicate(
    state: &AppState,
    paths: Vec<String>,
    op_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let cancel = match &op_id {
        Some(id) => register_op(id),
        None => CancelToken::new(),
    };
    let out = run_op(&sink, &op_id, "duplicate", |sink| {
        copy::duplicate(&paths, sink, &cancel)
    });
    if let Some(id) = &op_id {
        unregister_op(id);
    }
    out
}
