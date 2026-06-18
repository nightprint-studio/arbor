//! Cloud-storage domain — platform-backend handlers (Wave 1).
//!
//! Migrated from `crate::commands::cloud_commands`. These are the
//! **host-independent** commands: keyring secrets, stateless ops, cancellation
//! flags, and progress/done reporters. The host-dependent transfer commands
//! (`cloud_download`, `cloud_upload`, `cloud_sync`, `cloud_download_many`,
//! `cloud_list_stream`, `cloud_search_stream`, `cloud_gcs_oauth_start`) are
//! deferred to Wave 3 and remain in `cloud_commands.rs`.
//!
//! Every handler here takes `state: &AppState` (no `AppHandle`, no Tauri State
//! generics). Event emission (`cloud_report_progress`, `cloud_report_done`) goes
//! through `state.emit(topic, payload)`.

use crate::cloud::{
    self,
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
