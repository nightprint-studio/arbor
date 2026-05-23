//! Tauri commands backing the cloud-storage plugin.
//!
//! Stateless: every call carries a full `CloudConnection`. Secrets live in
//! the host's keyring (see `cloud::secrets`), keyed by an opaque `secret_ref`
//! the plugin chose. The host never persists anything else.
//!
//! Cloud logic lives in `crates/arbor-cloud`. These commands are thin
//! shims that pull the `Arc<dyn CloudHost>` out of Tauri State and forward
//! into the crate. The `CloudHost` is published once at startup by
//! `crate::cloud::install` (see `cloud/mod.rs`).

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use arbor_cloud::host::CloudHost;
use crate::cloud::{
    self,
    types::{CloudConnection, CloudListPage, CloudObject, CloudTestReport},
    transfer::SyncDir,
};
use crate::error::{AppError, Result};
use crate::AppState;

// ── Secrets (keyring) ──────────────────────────────────────────────────────

#[tauri::command]
pub fn cloud_secret_set(_state: State<'_, AppState>, secret_ref: String, value: String) -> Result<()> {
    cloud::secrets::set(&secret_ref, &value).map_err(Into::into)
}

#[tauri::command]
pub fn cloud_secret_exists(_state: State<'_, AppState>, secret_ref: String) -> Result<bool> {
    cloud::secrets::exists(&secret_ref).map_err(Into::into)
}

#[tauri::command]
pub fn cloud_secret_delete(_state: State<'_, AppState>, secret_ref: String) -> Result<()> {
    cloud::secrets::delete(&secret_ref).map_err(Into::into)
}

// ── Connection probe ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn cloud_test_connection(
    _state: State<'_, AppState>,
    conn:   CloudConnection,
    bucket: Option<String>,
) -> Result<CloudTestReport> {
    cloud::ops::test_connection(&conn, bucket.as_deref()).await.map_err(Into::into)
}

// ── Object operations ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn cloud_list(
    _state: State<'_, AppState>,
    conn:   CloudConnection,
    bucket: String,
    prefix: Option<String>,
    limit:  Option<usize>,
) -> Result<CloudListPage> {
    cloud::ops::list(&conn, &bucket, prefix.as_deref().unwrap_or(""), limit).await
        .map_err(Into::into)
}

/// Wildcard search — recursive list under `root_prefix` filtered by a glob
/// pattern. Streams matches via the same chunk-event mechanism as
/// `cloud_list_stream` so the plugin can reuse its accumulator. Pattern
/// semantics: `*` = same-segment wildcard, `**` = cross-segment, `?` = one
/// non-separator char. Capped at SEARCH_HARD_CAP results.
#[tauri::command]
pub async fn cloud_search_stream(
    host:        State<'_, Arc<dyn CloudHost>>,
    state:       State<'_, AppState>,
    conn:        CloudConnection,
    bucket:      String,
    root_prefix: Option<String>,
    pattern:     String,
    stream_id:   String,
) -> Result<String> {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut map = state.cloud_cancellations.lock().map_err(|e|
            AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
        )?;
        map.insert(stream_id.clone(), cancel.clone());
    };
    let root  = root_prefix.unwrap_or_default();
    let sid   = stream_id.clone();
    let host  = host.inner().clone();
    let state_cancel = state.cloud_cancellations.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud::ops::search_stream(host, conn, bucket, root, pattern, sid.clone(), cancel).await;
        if let Ok(mut map) = state_cancel.lock() {
            map.remove(&sid);
        };
    });
    Ok(stream_id)
}

/// Streaming variant of `cloud_list` — emits `arbor://cloud-list-chunk`
/// events as opendal pages through the listing. Returns immediately with
/// the stream_id (so callers can cancel via `cloud_cancellations`).
#[tauri::command]
pub async fn cloud_list_stream(
    host:      State<'_, Arc<dyn CloudHost>>,
    state:     State<'_, AppState>,
    conn:      CloudConnection,
    bucket:    String,
    prefix:    Option<String>,
    stream_id: String,
    cap:       Option<usize>,
) -> Result<String> {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut map = state.cloud_cancellations.lock().map_err(|e|
            AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
        )?;
        map.insert(stream_id.clone(), cancel.clone());
    }
    let prefix = prefix.unwrap_or_default();
    let sid    = stream_id.clone();
    let host   = host.inner().clone();
    let state_cancel = state.cloud_cancellations.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud::ops::list_stream(host, conn, bucket, prefix, sid.clone(), cap, cancel).await;
        // Drop the cancellation flag from the registry once we're done.
        if let Ok(mut map) = state_cancel.lock() {
            map.remove(&sid);
        };
    });
    Ok(stream_id)
}

#[tauri::command]
pub async fn cloud_stat(
    _state: State<'_, AppState>,
    conn:   CloudConnection,
    bucket: String,
    path:   String,
) -> Result<CloudObject> {
    cloud::ops::stat(&conn, &bucket, &path).await.map_err(Into::into)
}

#[tauri::command]
pub async fn cloud_delete(
    _state:    State<'_, AppState>,
    conn:      CloudConnection,
    bucket:    String,
    path:      String,
    recursive: Option<bool>,
) -> Result<()> {
    cloud::ops::delete(&conn, &bucket, &path, recursive.unwrap_or(false)).await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn cloud_copy(
    _state: State<'_, AppState>,
    conn:   CloudConnection,
    bucket: String,
    src:    String,
    dst:    String,
) -> Result<()> {
    cloud::ops::copy(&conn, &bucket, &src, &dst).await.map_err(Into::into)
}

// ── Transfers (jobified) ───────────────────────────────────────────────────

#[tauri::command]
pub async fn cloud_download(
    host:   State<'_, Arc<dyn CloudHost>>,
    _state: State<'_, AppState>,
    conn:   CloudConnection,
    bucket: String,
    path:   String,
    local:  String,
) -> Result<String> {
    cloud::transfer::download(host.inner().clone(), conn, bucket, path, PathBuf::from(local))
        .await.map_err(Into::into)
}

#[tauri::command]
pub async fn cloud_upload(
    host:      State<'_, Arc<dyn CloudHost>>,
    _state:    State<'_, AppState>,
    conn:      CloudConnection,
    bucket:    String,
    path:      String,
    local:     String,
    overwrite: Option<bool>,
) -> Result<String> {
    cloud::transfer::upload(host.inner().clone(), conn, bucket, path, PathBuf::from(local), overwrite.unwrap_or(false))
        .await.map_err(Into::into)
}

#[tauri::command]
pub async fn cloud_sync(
    host:          State<'_, Arc<dyn CloudHost>>,
    _state:        State<'_, AppState>,
    conn:          CloudConnection,
    bucket:        String,
    remote_prefix: String,
    local:         String,
    direction:     String, // "up" | "down"
    delete:        Option<bool>,
) -> Result<String> {
    let dir = match direction.as_str() {
        "up"   => SyncDir::Up,
        "down" => SyncDir::Down,
        other  => return Err(AppError::Other(format!(
            "cloud_sync: direction must be \"up\" or \"down\", got {other:?}"
        ))),
    };
    cloud::transfer::sync(host.inner().clone(), conn, bucket, remote_prefix, PathBuf::from(local), dir, delete.unwrap_or(false))
        .await.map_err(Into::into)
}

// ── download_many / concat_files / is_cancelled ───────────────────────────

#[tauri::command]
pub async fn cloud_download_many(
    host:        State<'_, Arc<dyn CloudHost>>,
    _state:      State<'_, AppState>,
    conn:        CloudConnection,
    bucket:      String,
    paths:       Vec<String>,
    local_dir:   String,
    parallel:    Option<usize>,
    op_label:    Option<String>,
    stream_id:   String,
    extra_steps: Option<Vec<(String, String)>>,
    keep_open:   Option<bool>,
) -> Result<String> {
    let parallel = parallel.unwrap_or(4).clamp(1, 16);
    let op_label = op_label.unwrap_or_else(|| format!("Downloading {} files", paths.len()));
    cloud::transfer::download_many(
        host.inner().clone(), conn, bucket, paths, std::path::PathBuf::from(local_dir),
        parallel, op_label, stream_id,
        extra_steps.unwrap_or_default(),
        keep_open.unwrap_or(false),
    ).await.map_err(Into::into)
}

#[tauri::command]
pub async fn cloud_concat_files(
    _state:        State<'_, AppState>,
    inputs:        Vec<String>,
    output:        String,
    delete_inputs: Option<bool>,
) -> Result<()> {
    cloud::ops::concat_files(inputs, output, delete_inputs.unwrap_or(false)).await
        .map_err(Into::into)
}

/// Push a step update to the OperationsOverlay card backing a
/// `download_many` call. Used by the chunk-merge orchestrator to activate
/// the appended "merge" step (status=None → emit `set_current`) or to
/// mark intermediate sub-states (status=Some → emit `update_step`).
///
/// op_id is derived from `stream_id` the same way `run_download_many`
/// derives it (`cloud-storage:op:{stream_id}`), so the same card is
/// addressed end-to-end.
#[tauri::command]
pub fn cloud_report_progress(
    app:       tauri::AppHandle,
    _state:    State<'_, AppState>,
    stream_id: String,
    step:      String,
    status:    Option<String>,
    detail:    Option<String>,
) -> Result<()> {
    use tauri::Emitter;
    let op_id = format!("cloud-storage:op:{stream_id}");
    let kind = if status.is_some() { "update_step" } else { "set_current" };
    let _ = app.emit("arbor://plugin-operation-update", serde_json::json!({
        "id":     op_id,
        "plugin": "cloud-storage",
        "kind":   kind,
        "step":   step,
        "status": status,
        "detail": detail,
    }));
    Ok(())
}

/// Close the OperationsOverlay card for a `download_many` call whose
/// download phase was started with `keep_open=true`. Also finalizes the
/// JobRegistry entry stashed in `cloud_pending_ops`. No-op when no entry
/// is pending (defensive against double-fires from the chunk handler).
#[tauri::command]
pub fn cloud_report_done(
    app:       tauri::AppHandle,
    state:     State<'_, AppState>,
    stream_id: String,
    ok:        bool,
    summary:   Option<String>,
    error:     Option<String>,
) -> Result<()> {
    use tauri::Emitter;
    let op_id = format!("cloud-storage:op:{stream_id}");
    let _ = app.emit("arbor://plugin-operation-finish", serde_json::json!({
        "id":      op_id,
        "plugin":  "cloud-storage",
        "summary": summary,
        "error":   error,
    }));

    // Finalize the JobRegistry entry for the deferred download_many.
    let job_id = state.cloud_pending_ops.lock().ok()
        .and_then(|mut m| m.remove(&stream_id));
    if let Some(job_id) = job_id {
        // Did the user cancel during merge? Inspect the shared flag before
        // we drop it so the JobRegistry entry settles as Cancelled (not
        // Failed) — keeps the badge / overlay rendering consistent with
        // the cancellation path used in the pure-download case.
        let cancelled = state.cloud_cancellations.lock().ok()
            .and_then(|m| m.get(&stream_id).cloned())
            .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false);
        if let Ok(mut jobs) = state.lock_jobs() {
            let status = if ok {
                crate::jobs::JobStatus::Completed { exit_code: 0 }
            } else if cancelled {
                crate::jobs::JobStatus::Cancelled
            } else {
                crate::jobs::JobStatus::Failed {
                    error: error.clone().unwrap_or_else(|| "merge failed".into()),
                }
            };
            jobs.set_status(&job_id, status);
        }
        let final_err = if ok {
            None
        } else if cancelled {
            Some("cancelled".to_string())
        } else {
            error.clone().or_else(|| Some("merge failed".into()))
        };
        let _ = app.emit("arbor://job-done", serde_json::json!({
            "job_id":    job_id,
            "success":   ok,
            "exit_code": if ok { 0 } else { -1 },
            "cancelled": cancelled,
            "error":     final_err,
        }));
        if let Ok(mut map) = state.cloud_cancellations.lock() {
            map.remove(&job_id);
            map.remove(&stream_id);
        }
    }
    Ok(())
}

/// Flip the cooperative-cancel flag for a stream/aggregate id. Mirrors the
/// `arbor.cloud.cancel` Lua API but reachable from the frontend modal which
/// has no plugin VM. No-op when the id is unknown.
#[tauri::command]
pub fn cloud_cancel(state: State<'_, AppState>, stream_id: String) -> Result<()> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    if let Some(flag) = map.get(&stream_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub fn cloud_is_cancelled(state: State<'_, AppState>, stream_id: String) -> Result<bool> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    Ok(map.get(&stream_id)
        .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false))
}

// ── OAuth (Google installed-app, loopback :7732) ──────────────────────────

#[tauri::command]
pub async fn cloud_gcs_oauth_start(
    host:          State<'_, Arc<dyn CloudHost>>,
    _state:        State<'_, AppState>,
    secret_ref:    String,
    client_id:     String,
    client_secret: Option<String>,
) -> Result<String> {
    cloud::oauth_google::start(host.inner().clone(), secret_ref, client_id, client_secret)
        .await.map_err(Into::into)
}
