//! models_download — job-tracked download / delete of the ONNX transcription
//! models (basic-pitch, Demucs), fetched on-demand so the base bundle stays light.
//!
//! Ported from `src-tauri/src/merula/models.rs`'s download/delete commands. Each
//! model is a single `.onnx` file streamed to `<merula-data>/models/<file>` with
//! throttled progress, tracked as a job in the shell's `JobRegistry` over the
//! reverse channel via [`JobHandle`] (hidden + routed to the merula window). The
//! model **listing** (`merula_models`) lives in the sibling `models` domain
//! (`crate::models`); this module owns only the network-bearing ops.
//!
//! The streaming runs on a detached `std::thread` with its own current-thread
//! Tokio runtime (this crate has no shared runtime handle in `MerulaState`, and the
//! audio RT thread / dispatcher worker must never block on IO) — the same
//! `Runtime::new()` fallback shape corvus-be uses for off-runtime async.

use std::path::{Path, PathBuf};

use merula_core::config as config_cmds;
use crate::jobs::{category, percent_of, JobHandle, ProgressThrottle};
use crate::models;
use merula_core::prelude::MerulaState;

// ── Commands ──────────────────────────────────────────────────────────────────

/// Start a background download of model `id`. Returns the job id; progress flows
/// via `arbor://job-progress` / `job-done` (routed to the merula window).
#[arbor_rpc::handler]
fn merula_download_model(ctx: &MerulaState, id: String) -> Result<String, String> {
    let cfg = config_cmds::load();
    let m = models::desc(&id).ok_or_else(|| format!("unknown model `{id}`"))?;
    let url = models::url_for(&cfg, &id).ok_or_else(|| format!("no URL for model `{id}`"))?;
    let dest = models::models_dir(&cfg).join(m.filename);
    start_download(ctx, &id, m.name, &url, dest)
}

/// Delete a downloaded model file. No-op when already absent.
#[arbor_rpc::handler]
fn merula_delete_model(_ctx: &MerulaState, id: String) -> Result<(), String> {
    let cfg = config_cmds::load();
    let path = models::model_path(&cfg, &id).ok_or_else(|| format!("unknown model `{id}`"))?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ── Download plumbing (single file) ───────────────────────────────────────────

/// Register the job + spawn the streaming worker; return the job id immediately.
fn start_download(
    ctx: &MerulaState,
    id: &str,
    name: &str,
    url: &str,
    dest: PathBuf,
) -> Result<String, String> {
    let host = ctx
        .host_caller()
        .ok_or_else(|| "merula_download_model: no reverse channel".to_string())?;
    let job = JobHandle::register(
        host,
        ctx.event_sink(),
        &format!("Download {name}"),
        &format!("download model {id}"),
        category::DOWNLOADS,
    )?;
    let job_id = job.id.clone();

    let url = url.to_string();
    let spawn = std::thread::Builder::new()
        .name(format!("merula-model-dl-{job_id}"))
        .spawn(move || {
            let outcome = run_download(&job, &url, &dest);
            match outcome {
                Ok(DownloadOutcome::Completed) => job.finish_ok(),
                Ok(DownloadOutcome::Cancelled) => job.finish_cancelled(),
                Err(e) => job.finish_failed(e),
            }
        });
    if let Err(e) = spawn {
        return Err(format!("failed to spawn model-download thread: {e}"));
    }
    Ok(job_id)
}

/// Terminal outcome of a model download.
enum DownloadOutcome {
    Completed,
    /// The user stopped it via `cancel_job`; the `.part` temp was removed.
    Cancelled,
}

/// Drive the async stream on a fresh current-thread runtime (the worker thread has
/// no ambient reactor; building one per download is cheap and keeps this self-
/// contained without a shared runtime handle in `MerulaState`).
fn run_download(job: &JobHandle, url: &str, dest: &Path) -> Result<DownloadOutcome, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("runtime: {e}"))?;
    rt.block_on(download_file(job, url, dest))
}

/// Stream `url` to `dest` (via a `.part` temp + rename), emitting throttled progress
/// on the job. Polls `is_cancelled` per chunk so `cancel_job` stops the download
/// (the partial `.part` is removed) instead of running to completion.
async fn download_file(
    job: &JobHandle,
    url: &str,
    dest: &Path,
) -> Result<DownloadOutcome, String> {
    use futures_util::StreamExt;
    use std::io::Write;

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create models dir: {e}"))?;
    }
    let tmp = dest.with_extension("part");

    // `download_client` (not `client`): the API client's 30s TOTAL deadline spans the
    // body too, so it aborts every large model mid-stream.
    let resp = arbor_core::prelude::download_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("server error: {e}"))?;
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&tmp).map_err(|e| format!("create file: {e}"))?;
    let mut received: u64 = 0;
    let mut throttle = ProgressThrottle::default();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if job.is_cancelled() {
            drop(file);
            let _ = std::fs::remove_file(&tmp);
            return Ok(DownloadOutcome::Cancelled);
        }
        let chunk = chunk.map_err(|e| format!("download interrupted: {e}"))?;
        file.write_all(&chunk).map_err(|e| format!("write: {e}"))?;
        received += chunk.len() as u64;
        if throttle.should_emit(received, total) {
            job.emit_progress(percent_of(received, total) as i32);
        }
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    drop(file);
    std::fs::rename(&tmp, dest).map_err(|e| format!("finalize: {e}"))?;
    Ok(DownloadOutcome::Completed)
}
