//! Keep-shell Studio command: the background index refresh.
//!
//! The leaf-clean sidebar scanners and `.ron-studio.toml` mutators moved to the
//! `studio` Model-D backend (`crate::ipc::studio::{index, config}`). What stays
//! here is `studio_refresh_index` — it captures an `AppHandle`, fire-and-forget
//! spawns the heavy walk on the blocking pool, and streams progress through
//! Tauri events. None of that fits the sync `rpc` handler shape, so it remains a
//! Tauri command on the shell.

use crate::AppState;
use crate::error::AppError;
use crate::studio::index;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

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
/// streamed through Tauri events:
///
///   * `arbor://studio-index-progress` — `IndexProgress { tab_id, processed, total }`
///     emitted every ~25 files (or every file if total < 50).
///   * `arbor://studio-index-done`     — `IndexDone     { tab_id, files_indexed, took_ms }`
///     emitted exactly once when the walk finishes (success OR error).
///
/// The frontend's `studioStore` listens to these and surfaces a small
/// progress badge in the Studio sidebar.
#[tauri::command]
pub async fn studio_refresh_index(
    app:    AppHandle,
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<(), AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let app_clone = app.clone();
    let tab_id_clone = tab_id.clone();
    // Fire-and-forget — the heavy walk runs on the blocking pool so the
    // Tauri runtime stays responsive. We deliberately do NOT await the
    // join handle here: the IPC call resolves immediately, the job
    // emits events as it goes.
    tokio::task::spawn_blocking(move || {
        let started = std::time::Instant::now();
        let mut last_emit = 0usize;
        let mut cb: Box<index::ProgressFn> = {
            let app = app_clone.clone();
            let tab_id = tab_id_clone.clone();
            Box::new(move |processed: usize, total: usize| {
                // Coarse throttling — emitting on every file kills the
                // frontend event queue. Threshold scales down for small
                // repos so the user still sees progress feedback.
                let step = if total < 50 { 1 } else { (total / 40).max(1) };
                if processed - last_emit >= step || processed == total {
                    last_emit = processed;
                    let _ = app.emit("arbor://studio-index-progress", IndexProgress {
                        tab_id:    tab_id.clone(),
                        processed,
                        total,
                    });
                }
            })
        };
        let result = index::refresh(&repo_path, Some(&mut *cb));
        let files_indexed = result.as_ref().map(|i| i.files.len()).unwrap_or(0);
        let _ = app_clone.emit("arbor://studio-index-done", IndexDone {
            tab_id:    tab_id_clone,
            files_indexed,
            took_ms:   started.elapsed().as_millis() as u64,
        });
        if let Err(e) = result {
            tracing::warn!("studio index refresh failed: {e}");
        }
    });
    Ok(())
}
