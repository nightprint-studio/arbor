//! Commands powering the built-in Studio sidebar — a project-wide
//! `.ron`/`.json`/`.toml` index. The walk runs on a blocking thread so a
//! large repo doesn't stall the Tauri main runtime.

use crate::AppState;
use crate::error::AppError;
use crate::studio::{
    config::{self as studio_config, StudioConfig},
    find_usages_for, index, scan_broken_refs_for, scan_cross_refs_for, scan_repo,
    BrokenRef, CrossRefDef, StudioFileEntry, StudioFileKind, UsageMatch,
};
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

/// Scan the active tab's repository for indexable data files. `kinds`
/// filters the result: empty vec means "all supported kinds".
#[tauri::command]
pub async fn studio_scan_repo(
    state:  State<'_, AppState>,
    tab_id: String,
    kinds:  Vec<StudioFileKind>,
) -> Result<Vec<StudioFileEntry>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || scan_repo(&repo_path, &kinds))
        .await
        .map_err(|e| AppError::Other(format!("studio_scan_repo task panicked: {e}")))?
}

/// Project-wide cross-reference scan. Returns every `id: "…"` /
/// `name: "…"` definition across the active repo's files (RON / JSON
/// — `kinds` defaults to `[Ron]` when empty for back-compat). The
/// frontend folds the list into a `Map<id, Vec<def>>` per kind so a
/// single id duplicated across two files surfaces both targets in the
/// Ctrl+click picker.
#[tauri::command]
pub async fn studio_scan_cross_refs(
    state:  State<'_, AppState>,
    tab_id: String,
    kinds:  Option<Vec<StudioFileKind>>,
) -> Result<Vec<CrossRefDef>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state.lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    // Empty list = back-compat RON-only; explicit list = filter.
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    tokio::task::spawn_blocking(move || {
        if use_index {
            let idx = index::load(&repo_path);
            if !idx.files.is_empty() {
                return Ok(index::aggregate_cross_refs_for(&idx, &kinds));
            }
        }
        scan_cross_refs_for(&repo_path, &kinds)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_scan_cross_refs task panicked: {e}")))?
}

/// Reverse navigation: given a top-level `id`/`name` value, find every
/// reference field across the project pointing at it. Drives the
/// "Used by N files" panel on definition nodes.
#[tauri::command]
pub async fn studio_find_usages(
    state:  State<'_, AppState>,
    tab_id: String,
    target: String,
    kinds:  Option<Vec<StudioFileKind>>,
) -> Result<Vec<UsageMatch>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state.lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    tokio::task::spawn_blocking(move || {
        if use_index {
            let idx = index::load(&repo_path);
            if !idx.files.is_empty() {
                return Ok(index::aggregate_usages_for(&idx, &target, &kinds));
            }
        }
        find_usages_for(&repo_path, &target, &kinds)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_find_usages task panicked: {e}")))?
}

/// Project-wide broken-reference scan. Walks every `.ron` file in the
/// repo (skipping excludes), gathers every `id`/`name` definition,
/// and emits every reference whose value doesn't appear in that
/// def set — useful for catching renamed/deleted entities before
/// they ship as silently-dead pointers at runtime. Result is sorted
/// by orphan value so the same broken target groups visually.
#[tauri::command]
pub async fn studio_scan_broken_refs(
    state:  State<'_, AppState>,
    tab_id: String,
    kinds:  Option<Vec<StudioFileKind>>,
) -> Result<Vec<BrokenRef>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let use_index = state.lock_config()
        .map(|c| c.studio.use_index)
        .unwrap_or(false);
    let kinds = kinds.unwrap_or_else(|| vec![StudioFileKind::Ron]);
    tokio::task::spawn_blocking(move || {
        if use_index {
            let idx = index::load(&repo_path);
            if !idx.files.is_empty() {
                return Ok(index::aggregate_broken_refs_for(&idx, &kinds));
            }
        }
        scan_broken_refs_for(&repo_path, &kinds)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_scan_broken_refs task panicked: {e}")))?
}

/// Read the repo-root `.ron-studio.toml`. Returns an empty config when
/// the file is missing — useful for the sidebar to seed its UI state
/// without a separate "exists?" round-trip.
#[tauri::command]
pub async fn studio_get_config(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<StudioConfig, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || studio_config::load(&repo_path))
        .await
        .map_err(|e| AppError::Other(format!("studio_get_config task panicked: {e}")))?
}

/// Register an external location for the active project. `path` can
/// be a single file or a folder (validated server-side); `label` is
/// an optional human name used for the synthetic
/// `external/<label>/…` prefix in the sidebar tree and binding
/// globs. When omitted, the basename of `path` is used. Idempotent
/// on `path` — re-adding the same absolute path refreshes the label.
#[tauri::command]
pub async fn studio_add_external(
    state:  State<'_, AppState>,
    tab_id: String,
    path:   String,
    label:  Option<String>,
) -> Result<(), AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        studio_config::add_external(&mut cfg, &path, label.as_deref());
        studio_config::save(&repo_path, &cfg)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_add_external task panicked: {e}")))?
}

/// Drop an external location by `path`. No-op when the entry isn't
/// there; returns `true` when an entry was actually removed so the
/// frontend can skip the rescan in the unchanged case.
#[tauri::command]
pub async fn studio_remove_external(
    state:  State<'_, AppState>,
    tab_id: String,
    path:   String,
) -> Result<bool, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<bool, AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        let removed = studio_config::remove_external(&mut cfg, &path);
        if removed { studio_config::save(&repo_path, &cfg)?; }
        Ok(removed)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_remove_external task panicked: {e}")))?
}

/// Toggle an exclude entry for a single file (by repo-relative path).
/// Returns the new state — `true` means now excluded.
#[tauri::command]
pub async fn studio_toggle_exclude(
    state:         State<'_, AppState>,
    tab_id:        String,
    relative_path: String,
) -> Result<bool, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<bool, AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        let now = studio_config::toggle_exclude(&mut cfg, &relative_path);
        studio_config::save(&repo_path, &cfg)?;
        Ok(now)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_toggle_exclude task panicked: {e}")))?
}

/// Bind a `.rs` schema + root type to a single file (a per-file
/// override in `.ron-studio.toml`). The next scan / next time the file
/// is opened in RON Studio, this binding takes effect.
#[tauri::command]
pub async fn studio_bind_schema(
    state:            State<'_, AppState>,
    tab_id:           String,
    relative_path:    String,
    rs_file:          String,
    root_type:        String,
    // When `Some`, replaces the entry's stored reference-field patterns;
    // when `None`, the existing list (if any) is preserved so re-binding
    // via the UI doesn't wipe hand-curated patterns.
    reference_fields: Option<Vec<String>>,
) -> Result<(), AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        studio_config::set_binding(&mut cfg, &relative_path, &rs_file, &root_type, reference_fields);
        studio_config::save(&repo_path, &cfg)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_bind_schema task panicked: {e}")))?
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

/// Toggle a single field name in the reference-field patterns of the
/// override matching `relative_path`. Returns the new state.
///
/// Used by the RON Studio tree's "Mark/Unmark as reference field"
/// context-menu — lets the user define cross-ref keys without leaving
/// the tree view. Falls through to creating a per-file override when
/// no binding exists yet so the next save / index refresh picks it up.
#[tauri::command]
pub async fn studio_toggle_reference_field(
    state:         State<'_, AppState>,
    tab_id:        String,
    relative_path: String,
    field:         String,
) -> Result<bool, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<bool, AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        let (now, _scope) = studio_config::toggle_reference_field(&mut cfg, &relative_path, &field);
        studio_config::save(&repo_path, &cfg)?;
        Ok(now)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_toggle_reference_field task panicked: {e}")))?
}

/// Inverse of `studio_bind_schema` — drops the per-file override.
/// Returns `true` when something was removed.
#[tauri::command]
pub async fn studio_unbind_schema(
    state:         State<'_, AppState>,
    tab_id:        String,
    relative_path: String,
) -> Result<bool, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || -> Result<bool, AppError> {
        let mut cfg = studio_config::load(&repo_path).unwrap_or_default();
        let removed = studio_config::clear_binding(&mut cfg, &relative_path);
        if removed { studio_config::save(&repo_path, &cfg)?; }
        Ok(removed)
    })
    .await
    .map_err(|e| AppError::Other(format!("studio_unbind_schema task panicked: {e}")))?
}
