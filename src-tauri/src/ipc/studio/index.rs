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
//! `studio_refresh_index` lives here too: it spawns the heavy walk on a
//! background thread and streams `arbor://studio-index-*` progress events
//! through the **event sink** instead of an `AppHandle`. The generic `rpc`
//! handler returns its `Ok(())` immediately while the spawned thread keeps
//! emitting — behavior is byte-identical to the old keep-shell command.

use std::sync::Arc;

use serde::Serialize;

use crate::error::AppError;
use crate::ipc::studio;
use crate::studio::{
    find_usages_for, index, scan_broken_refs_for, scan_cross_refs_for, scan_repo, BrokenRef,
    CrossRefDef, StudioFileEntry, StudioFileKind, UsageMatch,
};
use crate::AppState;

/// Whether the persistent Studio index is enabled. The `studio` config section
/// is owned by corvus-be (`corvus/config.toml`); read it back with a thin
/// partial-struct read. Defaults to `false` (index off) when the file/section
/// is absent.
fn studio_use_index() -> bool {
    #[derive(serde::Deserialize)]
    struct StudioProbe {
        #[serde(default)]
        use_index: bool,
    }
    crate::config::corvus_read::section::<StudioProbe>("studio")
        .map(|s| s.use_index)
        .unwrap_or(false)
}

/// Snapshot of the index state — emitted on every refresh tick + on
/// completion so the sidebar can render a "Indexing N/M…" badge.
#[derive(Debug, Clone, Serialize)]
pub struct IndexProgress {
    pub tab_id:    String,
    pub processed: usize,
    pub total:     usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexDone {
    pub tab_id:        String,
    pub files_indexed: usize,
    pub took_ms:       u64,
}

/// Trigger a background refresh of the persistent studio index. The
/// IPC call returns as soon as the job is spawned — progress is
/// streamed through frontend events:
///
///   * `arbor://studio-index-progress` — `IndexProgress { tab_id, processed, total }`
///     emitted every ~25 files (or every file if total < 50).
///   * `arbor://studio-index-done`     — `IndexDone     { tab_id, files_indexed, took_ms }`
///     emitted exactly once when the walk finishes (success OR error).
///
/// The frontend's `studioStore` listens to these and surfaces a small
/// progress badge in the Studio sidebar.
#[studio::handler(program = "studio")]
fn studio_refresh_index(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;

    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let sink_bg = Arc::clone(&sink);
    let tab_id_clone = tab_id.clone();
    // Fire-and-forget — the heavy walk runs on a background thread so the
    // calling worker stays responsive. We deliberately do NOT join the
    // handle here: the IPC call resolves immediately, the thread emits
    // events as it goes.
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let mut last_emit = 0usize;
        let mut cb: Box<index::ProgressFn> = {
            let sink = Arc::clone(&sink_bg);
            let tab_id = tab_id_clone.clone();
            Box::new(move |processed: usize, total: usize| {
                // Coarse throttling — emitting on every file kills the
                // frontend event queue. Threshold scales down for small
                // repos so the user still sees progress feedback.
                let step = if total < 50 { 1 } else { (total / 40).max(1) };
                if processed - last_emit >= step || processed == total {
                    last_emit = processed;
                    sink.emit(
                        "arbor://studio-index-progress",
                        serde_json::to_value(IndexProgress {
                            tab_id: tab_id.clone(),
                            processed,
                            total,
                        })
                        .unwrap_or(serde_json::Value::Null),
                    );
                }
            })
        };
        let result = index::refresh(&repo_path, Some(&mut *cb));
        let files_indexed = result.as_ref().map(|i| i.files.len()).unwrap_or(0);
        sink_bg.emit(
            "arbor://studio-index-done",
            serde_json::to_value(IndexDone {
                tab_id: tab_id_clone,
                files_indexed,
                took_ms: started.elapsed().as_millis() as u64,
            })
            .unwrap_or(serde_json::Value::Null),
        );
        if let Err(e) = result {
            tracing::warn!("studio index refresh failed: {e}");
        }
    });
    Ok(())
}

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
    let use_index = studio_use_index();
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
    let use_index = studio_use_index();
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
    let use_index = studio_use_index();
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    if use_index {
        let idx = index::load(&repo_path);
        if !idx.files.is_empty() {
            return Ok(index::aggregate_broken_refs_for(&idx, &kinds));
        }
    }
    scan_broken_refs_for(&repo_path, &kinds)
}
