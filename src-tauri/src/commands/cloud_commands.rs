//! Tauri commands backing the cloud-storage plugin — Wave 3 residue.
//!
//! The host-independent commands (keyring secrets, stateless ops, cancellation,
//! progress reporters) have been migrated to `crate::ipc::platform::cloud` as
//! `#[platform::handler]` functions (Wave 1). What remains here are the
//! **host-dependent** transfer commands that need an `Arc<dyn CloudHost>` pulled
//! from Tauri State — those are deferred to Wave 3 (WASM migration).
//!
//! Cloud logic lives in `crates/arbor-cloud`. These commands are thin
//! shims that pull the `Arc<dyn CloudHost>` out of Tauri State and forward
//! into the crate.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use arbor_cloud::host::CloudHost;
use crate::cloud::{
    self,
    types::CloudConnection,
    transfer::SyncDir,
};
use crate::error::{AppError, Result};
use crate::AppState;

// ── Transfers (jobified, host-dependent) ──────────────────────────────────

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
