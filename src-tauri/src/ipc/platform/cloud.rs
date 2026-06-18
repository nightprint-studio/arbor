//! Cloud-storage domain — platform-backend handlers (Wave 1 + Wave 3).
//!
//! Migrated from `crate::commands::cloud_commands`. Wave 1 covered the
//! **host-independent** commands: keyring secrets, stateless ops, cancellation
//! flags, and progress/done reporters. Wave 3 adds the **host-dependent**
//! transfer commands that need an `Arc<dyn CloudHost>`, now reachable via
//! `state.cloud_host()` (published in `cloud::install()`):
//! `cloud_download`, `cloud_upload`, `cloud_sync`, `cloud_download_many`,
//! `cloud_list_stream`, `cloud_search_stream`, `cloud_gcs_oauth_start`.
//!
//! Every handler here takes `state: &AppState` (no `AppHandle`, no Tauri State
//! generics). Event emission (`cloud_report_progress`, `cloud_report_done`) goes
//! through `state.emit(topic, payload)`.

use std::path::PathBuf;

use crate::cloud::{
    self,
    transfer::SyncDir,
    types::{CloudConnection, CloudListPage, CloudObject, CloudTestReport},
};
use crate::error::{AppError, Result};
use crate::ipc::platform;
use crate::AppState;

// ── Secrets (keyring) ─────────────────────────────────────────────────────

#[platform::handler(program = "platform")]
fn cloud_secret_set(_state: &AppState, secret_ref: String, value: String) -> Result<()> {
    cloud::secrets::set(&secret_ref, &value).map_err(Into::into)
}

#[platform::handler(program = "platform")]
fn cloud_secret_exists(_state: &AppState, secret_ref: String) -> Result<bool> {
    cloud::secrets::exists(&secret_ref).map_err(Into::into)
}

#[platform::handler(program = "platform")]
fn cloud_secret_delete(_state: &AppState, secret_ref: String) -> Result<()> {
    cloud::secrets::delete(&secret_ref).map_err(Into::into)
}

// ── Connection probe ──────────────────────────────────────────────────────

#[platform::handler(program = "platform")]
async fn cloud_test_connection(
    _state: &AppState,
    conn:   CloudConnection,
    bucket: Option<String>,
) -> Result<CloudTestReport> {
    cloud::ops::test_connection(&conn, bucket.as_deref()).await.map_err(Into::into)
}

// ── Object operations ─────────────────────────────────────────────────────

#[platform::handler(program = "platform")]
async fn cloud_list(
    _state: &AppState,
    conn:   CloudConnection,
    bucket: String,
    prefix: Option<String>,
    limit:  Option<usize>,
) -> Result<CloudListPage> {
    cloud::ops::list(&conn, &bucket, prefix.as_deref().unwrap_or(""), limit).await
        .map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_stat(
    _state: &AppState,
    conn:   CloudConnection,
    bucket: String,
    path:   String,
) -> Result<CloudObject> {
    cloud::ops::stat(&conn, &bucket, &path).await.map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_delete(
    _state:    &AppState,
    conn:      CloudConnection,
    bucket:    String,
    path:      String,
    recursive: Option<bool>,
) -> Result<()> {
    cloud::ops::delete(&conn, &bucket, &path, recursive.unwrap_or(false)).await
        .map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_copy(
    _state: &AppState,
    conn:   CloudConnection,
    bucket: String,
    src:    String,
    dst:    String,
) -> Result<()> {
    cloud::ops::copy(&conn, &bucket, &src, &dst).await.map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_concat_files(
    _state:        &AppState,
    inputs:        Vec<String>,
    output:        String,
    delete_inputs: Option<bool>,
) -> Result<()> {
    cloud::ops::concat_files(inputs, output, delete_inputs.unwrap_or(false)).await
        .map_err(Into::into)
}

// ── Cancellation ──────────────────────────────────────────────────────────

#[platform::handler(program = "platform")]
fn cloud_cancel(state: &AppState, stream_id: String) -> Result<()> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    if let Some(flag) = map.get(&stream_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[platform::handler(program = "platform")]
fn cloud_is_cancelled(state: &AppState, stream_id: String) -> Result<bool> {
    let map = state.cloud_cancellations.lock().map_err(|e|
        AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
    )?;
    Ok(map.get(&stream_id)
        .map(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
        .unwrap_or(false))
}

// ── Progress / done reporters ─────────────────────────────────────────────

/// Push a step update to the OperationsOverlay card backing a
/// `download_many` call. Used by the chunk-merge orchestrator to activate
/// the appended "merge" step (status=None → emit `set_current`) or to
/// mark intermediate sub-states (status=Some → emit `update_step`).
///
/// op_id is derived from `stream_id` the same way `run_download_many`
/// derives it (`cloud-storage:op:{stream_id}`), so the same card is
/// addressed end-to-end.
#[platform::handler(program = "platform")]
fn cloud_report_progress(
    state:     &AppState,
    stream_id: String,
    step:      String,
    status:    Option<String>,
    detail:    Option<String>,
) -> Result<()> {
    let op_id = format!("cloud-storage:op:{stream_id}");
    let kind = if status.is_some() { "update_step" } else { "set_current" };
    state.emit("arbor://plugin-operation-update", serde_json::json!({
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
#[platform::handler(program = "platform")]
fn cloud_report_done(
    state:     &AppState,
    stream_id: String,
    ok:        bool,
    summary:   Option<String>,
    error:     Option<String>,
) -> Result<()> {
    let op_id = format!("cloud-storage:op:{stream_id}");
    state.emit("arbor://plugin-operation-finish", serde_json::json!({
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
        state.emit("arbor://job-done", serde_json::json!({
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

// ── Transfers (Wave 3 — host-dependent) ──────────────────────────────────────

/// Helper: pull the cloud host from AppState, returning a clean error if it
/// hasn't been installed yet (only possible in the brief window before
/// `cloud::install()` runs at setup time).
macro_rules! get_host {
    ($state:expr) => {
        $state.cloud_host().ok_or_else(|| AppError::Other("cloud host not ready".into()))?
    };
}

#[platform::handler(program = "platform")]
async fn cloud_download(
    state:  &AppState,
    conn:   CloudConnection,
    bucket: String,
    path:   String,
    local:  String,
) -> Result<String> {
    let host = get_host!(state);
    cloud::transfer::download(host, conn, bucket, path, PathBuf::from(local))
        .await.map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_upload(
    state:     &AppState,
    conn:      CloudConnection,
    bucket:    String,
    path:      String,
    local:     String,
    overwrite: Option<bool>,
) -> Result<String> {
    let host = get_host!(state);
    cloud::transfer::upload(
        host, conn, bucket, path, PathBuf::from(local), overwrite.unwrap_or(false),
    ).await.map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_sync(
    state:         &AppState,
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
    let host = get_host!(state);
    cloud::transfer::sync(
        host, conn, bucket, remote_prefix, PathBuf::from(local), dir, delete.unwrap_or(false),
    ).await.map_err(Into::into)
}

#[platform::handler(program = "platform")]
async fn cloud_download_many(
    state:       &AppState,
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
    let host = get_host!(state);
    cloud::transfer::download_many(
        host, conn, bucket, paths, PathBuf::from(local_dir),
        parallel, op_label, stream_id,
        extra_steps.unwrap_or_default(),
        keep_open.unwrap_or(false),
    ).await.map_err(Into::into)
}

/// Streaming variant of `cloud_list` — emits `arbor://cloud-list-chunk`
/// events as opendal pages through the listing. Returns immediately with
/// the stream_id so callers can cancel via `cloud_cancel`.
#[platform::handler(program = "platform")]
async fn cloud_list_stream(
    state:     &AppState,
    conn:      CloudConnection,
    bucket:    String,
    prefix:    Option<String>,
    stream_id: String,
    cap:       Option<usize>,
) -> Result<String> {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Drop the guard before the first .await point.
    {
        let mut map = state.cloud_cancellations.lock().map_err(|e|
            AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
        )?;
        map.insert(stream_id.clone(), cancel.clone());
    }
    // Mirror the token into the generic stream registry so the seam's
    // `cancel_stream` handler can cancel this stream by id too.
    state.streams.insert(&stream_id, cancel.clone());
    let prefix          = prefix.unwrap_or_default();
    let sid             = stream_id.clone();
    let host            = get_host!(state);
    let state_cancel    = state.cloud_cancellations.clone();
    let streams         = state.streams.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud::ops::list_stream(host, conn, bucket, prefix, sid.clone(), cap, cancel).await;
        if let Ok(mut map) = state_cancel.lock() {
            map.remove(&sid);
        }
        streams.remove(&sid);
    });
    Ok(stream_id)
}

/// Wildcard search — recursive list under `root_prefix` filtered by a glob
/// pattern. Streams matches via `arbor://cloud-list-chunk` events so the
/// plugin can reuse its accumulator. Pattern semantics: `*` = same-segment
/// wildcard, `**` = cross-segment, `?` = one non-separator char. Capped at
/// SEARCH_HARD_CAP results.
#[platform::handler(program = "platform")]
async fn cloud_search_stream(
    state:       &AppState,
    conn:        CloudConnection,
    bucket:      String,
    root_prefix: Option<String>,
    pattern:     String,
    stream_id:   String,
) -> Result<String> {
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // Drop the guard before the first .await point.
    {
        let mut map = state.cloud_cancellations.lock().map_err(|e|
            AppError::MutexPoisoned(format!("cloud_cancellations: {e}"))
        )?;
        map.insert(stream_id.clone(), cancel.clone());
    }
    // Mirror the token into the generic stream registry (see cloud_list_stream).
    state.streams.insert(&stream_id, cancel.clone());
    let root            = root_prefix.unwrap_or_default();
    let sid             = stream_id.clone();
    let host            = get_host!(state);
    let state_cancel    = state.cloud_cancellations.clone();
    let streams         = state.streams.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud::ops::search_stream(host, conn, bucket, root, pattern, sid.clone(), cancel).await;
        if let Ok(mut map) = state_cancel.lock() {
            map.remove(&sid);
        }
        streams.remove(&sid);
    });
    Ok(stream_id)
}

// ── OAuth (Google installed-app, loopback :7732) ──────────────────────────────

#[platform::handler(program = "platform")]
async fn cloud_gcs_oauth_start(
    state:         &AppState,
    secret_ref:    String,
    client_id:     String,
    client_secret: Option<String>,
) -> Result<String> {
    let host = get_host!(state);
    cloud::oauth_google::start(host, secret_ref, client_id, client_secret)
        .await.map_err(Into::into)
}
