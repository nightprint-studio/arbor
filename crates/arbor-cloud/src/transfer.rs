//! Streaming download/upload + recursive sync.
//!
//! Each entry point registers a job via [`CloudHost::job_register`], spawns
//! a tokio task, and returns the job id immediately. The task streams bytes
//! in chunks, periodically:
//!   * emits `arbor://cloud-progress` via [`CloudHost::emit_event`] (the JS
//!     frontend listens)
//!   * fires the `cloud-storage:progress` plugin hook via
//!     [`CloudHost::fire_plugin_hook`] (Lua subscribers listen)
//!   * appends a human-readable line via [`CloudHost::job_append_output`]
//! and on completion fires `arbor://cloud-job-done` + sets the final
//! `CloudJobStatus`.
//!
//! Cancellation is cooperative: every spawn registers an `Arc<AtomicBool>`
//! in [`CloudHost::cancellations`]. The host's `cancel_job` Tauri command
//! flips that flag (in addition to the normal PID-kill path used for
//! subprocess-backed jobs) and the streaming loop breaks out at the next
//! chunk boundary.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{CloudError, Result};
use crate::host::{CloudHost, CloudJobInfo, CloudJobStatus};
use crate::operator::{build, map_op_err};
use crate::types::{CloudConnection, CloudProgress};

const PLUGIN_NAME:    &str = "cloud-storage";
const HOOK_PROGRESS:  &str = "cloud-storage:progress";
const HOOK_JOB_DONE:  &str = "cloud-storage:job-done";

// download_many → modal listens on the Tauri event channel, Lua plugin
// orchestrator listens on the hook (only the terminal `done` event reaches
// both — progress updates are UI-only and don't need Lua delivery).
const EVT_MANY_PROGRESS: &str = "arbor://cloud-many-progress";
const EVT_MANY_DONE:     &str = "arbor://cloud-many-done";
const HOOK_MANY_DONE:    &str = "cloud-storage:download-many-done";

/// Helper: serialise a `Value` and fan it out to Lua subscribers via the
/// `cloud-storage` plugin hook. Errors during encoding are logged and
/// swallowed — there's nothing the caller could do with them.
fn fire_plugin_hook(host: &dyn CloudHost, hook: &str, payload: serde_json::Value) {
    let json = match serde_json::to_string(&payload) {
        Ok(s)  => s,
        Err(e) => { tracing::warn!("cloud hook encode: {e}"); return; }
    };
    host.fire_plugin_hook(PLUGIN_NAME, hook, &json);
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

const CHUNK_SIZE:        usize = 256 * 1024;  // 256 KiB per opendal read/write
const PROGRESS_TICK_MS:  u128  = 200;          // emit/append every ~200ms

// ── public entry points ────────────────────────────────────────────────────

pub async fn download(
    host:      Arc<dyn CloudHost>,
    conn:      CloudConnection,
    bucket:    String,
    remote:    String,
    local:     PathBuf,
) -> Result<String> {
    let (job_id, cancel) = spawn_job_shell(
        &*host,
        &conn.config_id,
        "Download",
        &format!("{bucket}:{remote} → {}", local.display()),
    )?;
    let host_t = host.clone();
    let task_id = job_id.clone();
    tokio::spawn(async move {
        let result = run_download(
            &*host_t, &task_id, &conn, &bucket, &remote, &local, cancel.clone()
        ).await;
        finalize_job(&*host_t, &task_id, result, &cancel).await;
    });
    Ok(job_id)
}

pub async fn upload(
    host:      Arc<dyn CloudHost>,
    conn:      CloudConnection,
    bucket:    String,
    remote:    String,
    local:     PathBuf,
    overwrite: bool,
) -> Result<String> {
    let (job_id, cancel) = spawn_job_shell(
        &*host,
        &conn.config_id,
        "Upload",
        &format!("{} → {bucket}:{remote}", local.display()),
    )?;
    let host_t = host.clone();
    let task_id = job_id.clone();
    tokio::spawn(async move {
        let result = run_upload(
            &*host_t, &task_id, &conn, &bucket, &remote, &local, overwrite, cancel.clone()
        ).await;
        finalize_job(&*host_t, &task_id, result, &cancel).await;
    });
    Ok(job_id)
}

pub async fn sync(
    host:      Arc<dyn CloudHost>,
    conn:      CloudConnection,
    bucket:    String,
    remote_prefix: String,
    local:     PathBuf,
    direction: SyncDir,
    delete:    bool,
) -> Result<String> {
    let kind_label = match direction { SyncDir::Down => "Sync ↓", SyncDir::Up => "Sync ↑" };
    let label = match direction {
        SyncDir::Down => format!("{bucket}:{remote_prefix} → {}", local.display()),
        SyncDir::Up   => format!("{} → {bucket}:{remote_prefix}", local.display()),
    };
    let (job_id, cancel) = spawn_job_shell(&*host, &conn.config_id, kind_label, &label)?;
    let host_t = host.clone();
    let task_id = job_id.clone();
    tokio::spawn(async move {
        let result = run_sync(
            &*host_t, &task_id, &conn, &bucket, &remote_prefix, &local,
            direction, delete, cancel.clone(),
        ).await;
        finalize_job(&*host_t, &task_id, result, &cancel).await;
    });
    Ok(job_id)
}

#[derive(Debug, Clone, Copy)]
pub enum SyncDir { Down, Up }

// ── download_many: N parallel sub-downloads as one aggregate job ───────────

const PROGRESS_AGGREGATE_MS: u128 = 250;

#[derive(Debug, Clone, serde::Serialize)]
struct ManyFileState {
    index:        usize,
    path:         String,
    basename:     String,
    local_path:   String,
    bytes_done:   u64,
    bytes_total:  u64,
    /// "queued" | "downloading" | "done" | "failed" | "cancelled"
    status:       &'static str,
    error:        Option<String>,
}

/// Append-after-files steps for the OperationsOverlay card. Used by the
/// chunk-merge flow so the same card can host both download and merge
/// phases. Each tuple is `(step_key, step_label)`.
pub type ExtraSteps = Vec<(String, String)>;

pub async fn download_many(
    host:       Arc<dyn CloudHost>,
    conn:       CloudConnection,
    bucket:     String,
    paths:      Vec<String>,
    local_dir:  PathBuf,
    parallel:   usize,
    op_label:   String,
    stream_id:  String,
    extra_steps: ExtraSteps,
    keep_open:  bool,
) -> Result<String> {
    if paths.is_empty() {
        return Err(CloudError::Other("download_many: paths is empty".into()));
    }
    tokio::fs::create_dir_all(&local_dir).await
        .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", local_dir.display())))?;

    // One shared cancellation flag for the aggregate. Registered in the
    // host's cancellations map so `cancel_job` / `arbor.cloud.cancel` flip
    // it just like any other cloud op.
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut map = host.cancellations().lock().map_err(|e|
            CloudError::Other(format!("cloud_cancellations poisoned: {e}"))
        )?;
        map.insert(stream_id.clone(), cancel.clone());
    };

    // Register the aggregate job so it appears in JobsOverlay / status bar
    // count, and `cancel_job` can flip the cancel flag through the standard
    // plumbing.
    let job_id = {
        let id = host.job_new_id();
        host.job_register(CloudJobInfo {
            id:          id.clone(),
            name:        op_label.clone(),
            plugin_name: "cloud-storage".into(),
            command:     "cloud:download_many".into(),
            started_at:  now_secs(),
            status:      CloudJobStatus::Running,
            category:    Some("Cloud Storage".into()),
            non_cancellable: false,
            hidden:      false,
            is_system:   false,
        });
        id
    };
    // Notify the frontend so the aggregate shows up in JobsOverlay with a
    // proper name. The single-shell helper emits the same payload shape;
    // download_many goes its own route (no spawn_job_shell call) so we have
    // to mirror it here.
    host.emit_event("arbor://job-started", serde_json::json!({
        "job_id":      job_id,
        "name":        op_label,
        "plugin_name": "cloud-storage",
        "command":     "cloud:download_many",
        "category":    "Cloud Storage",
        "hidden":      false,
        "stream_id":   stream_id,
    }));
    // Bind the job_id to the same cancel flag so the standard cancel_job
    // (which keys by job_id) hits this aggregate too.
    {
        if let Ok(mut map) = host.cancellations().lock() {
            map.insert(job_id.clone(), cancel.clone());
        };
    }

    // Per-file initial state. Naming: prefer the original basename; only
    // when that would collide with an earlier index do we add a zero-padded
    // index prefix. Keeps bulk-download outputs human-readable while still
    // working for chunks (whose order is conveyed via `local_paths` in the
    // done payload — on-disk naming doesn't matter to the chunk handler).
    let basenames: Vec<String> = paths.iter()
        .map(|p| p.rsplit('/').next().unwrap_or(p).to_string())
        .collect();
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let states: Arc<tokio::sync::Mutex<Vec<ManyFileState>>> = Arc::new(tokio::sync::Mutex::new(
        paths.iter().enumerate().map(|(i, p)| {
            let base = basenames[i].clone();
            let on_disk = if used.contains(&base) {
                format!("{:04}__{}", i, base)
            } else {
                base.clone()
            };
            used.insert(on_disk.clone());
            let local = local_dir.join(&on_disk);
            ManyFileState {
                index: i,
                path: p.clone(),
                basename: base,
                local_path: local.to_string_lossy().into_owned(),
                bytes_done: 0,
                bytes_total: 0,
                status: "queued",
                error: None,
            }
        }).collect()
    ));

    let host_for_task    = host.clone();
    let stream_for_task  = stream_id.clone();
    let job_for_task     = job_id.clone();
    let cancel_for_task  = cancel.clone();
    let states_for_task  = states.clone();
    let label_for_task   = op_label.clone();
    let extra_for_task   = extra_steps.clone();
    tokio::spawn(async move {
        run_download_many(
            host_for_task,
            conn,
            bucket,
            local_dir,
            parallel.max(1),
            stream_for_task,
            job_for_task,
            label_for_task,
            cancel_for_task,
            states_for_task,
            extra_for_task,
            keep_open,
        ).await;
    });

    Ok(job_id)
}

#[allow(clippy::too_many_arguments)]
async fn run_download_many(
    host:        Arc<dyn CloudHost>,
    conn:        CloudConnection,
    bucket:      String,
    _local_dir:  PathBuf,
    parallel:    usize,
    stream_id:   String,
    job_id:      String,
    op_label:    String,
    cancel:      Arc<AtomicBool>,
    states:      Arc<tokio::sync::Mutex<Vec<ManyFileState>>>,
    extra_steps: ExtraSteps,
    keep_open:   bool,
) {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(parallel));
    let n = { states.lock().await.len() };

    // ── OperationsOverlay card ─────────────────────────────────────────────
    // Reuses the same bottom-right card stack as Fetch-all / Pull-all so the
    // user sees a familiar progress UI (collapsed: title + "k/N file" + 1s
    // elapsed + slim progress bar; expanded: per-file step list). The id is
    // stream-id-based so the Lua orchestrator (which only has stream_id) can
    // address the same card from `report_progress` / `report_done` during
    // the chunk-merge phase.
    let op_id = format!("cloud-storage:op:{stream_id}");
    {
        let snap = states.lock().await.clone();
        let mut steps: Vec<serde_json::Value> = snap.iter()
            .map(|f| serde_json::json!({
                "key":   format!("f{}", f.index),
                "label": f.basename,
            }))
            .collect();
        // Append extra steps (e.g. a "merge" step) so they're visible on the
        // card from the start. The chunk-merge phase activates the "merge"
        // step via `arbor.cloud.report_progress` after downloads finish.
        for (key, label) in &extra_steps {
            steps.push(serde_json::json!({ "key": key, "label": label }));
        }
        host.emit_event("arbor://plugin-operation-start", serde_json::json!({
            "id":       &op_id,
            "plugin":   "cloud-storage",
            "title":    &op_label,
            "subtitle": format!("{} file{}", n, if n == 1 { "" } else { "s" }),
            "steps":    steps,
            "current":  if n > 0 { format!("f0") } else { String::new() },
        }));
    }

    // When the caller plans to drive a follow-up phase (chunk merge), we
    // stash the job_id so `report_done` can finalize the job once that
    // phase reports its terminal state.
    if keep_open {
        if let Ok(mut map) = host.pending_ops().lock() {
            map.insert(stream_id.clone(), job_id.clone());
        }
    }

    // Periodic aggregate-progress emitter. Reads the shared state snapshot
    // every PROGRESS_AGGREGATE_MS and fires the hook. Stops when `done` is set.
    let prog_host   = host.clone();
    let prog_states = states.clone();
    let prog_label  = op_label.clone();
    let prog_stream = stream_id.clone();
    let prog_done   = Arc::new(AtomicBool::new(false));
    let prog_done_w = prog_done.clone();
    let progress_task = tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(PROGRESS_AGGREGATE_MS as u64));
        loop {
            tick.tick().await;
            if prog_done_w.load(Ordering::Relaxed) { break; }
            emit_aggregate_progress(&*prog_host, &prog_stream, &prog_label, &prog_states).await;
        }
    });

    // Spawn one task per file, capped by the semaphore.
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        let sem      = semaphore.clone();
        let conn_c   = conn.clone();
        let bucket_c = bucket.clone();
        let states_c = states.clone();
        let cancel_c = cancel.clone();
        let host_c   = host.clone();
        // Clone the per-task strings BEFORE the `async move` closure so the
        // owner (`job_id` / `op_id`) survives every iteration. With `async
        // move` an inner `.clone()` would still move the original on the
        // first lap and the second iteration would fail to borrow it.
        let job_c    = job_id.clone();
        let op_c     = op_id.clone();

        let h = tokio::spawn(async move {
            // Snapshot path + local target before awaiting the semaphore;
            // releases the mutex quickly.
            let (path, local_path) = {
                let s = states_c.lock().await;
                (s[i].path.clone(), s[i].local_path.clone())
            };

            let step_key = format!("f{}", i);
            if cancel_c.load(Ordering::Relaxed) {
                let mut s = states_c.lock().await;
                s[i].status = "cancelled";
                let line = format!("[{:>3}/{}] cancelled: {}", i + 1, n, s[i].basename);
                host_c.job_append_output(&job_c, line);
                host_c.emit_event("arbor://plugin-operation-update", serde_json::json!({
                    "id": &op_c, "plugin": "cloud-storage",
                    "kind": "update_step", "step": &step_key,
                    "status": "skipped", "detail": "cancelled before start",
                }));
                return;
            }

            let permit = match sem.acquire().await {
                Ok(p)  => p,
                Err(_) => {
                    let mut s = states_c.lock().await;
                    s[i].status = "failed";
                    s[i].error  = Some("semaphore closed".into());
                    return;
                }
            };

            {
                let mut s = states_c.lock().await;
                s[i].status = "downloading";
                let line = format!("[{:>3}/{}] start: {}", i + 1, n, s[i].basename);
                host_c.job_append_output(&job_c, line);
                // Mark THIS step as the current active one — the collapsed
                // overlay header reads `currentIdx + 1 / N · <label>` from this.
                host_c.emit_event("arbor://plugin-operation-update", serde_json::json!({
                    "id":     &op_c,
                    "plugin": "cloud-storage",
                    "kind":   "set_current",
                    "step":   &step_key,
                    "detail": s[i].basename,
                }));
            }

            // Stream this file. Updates per-file bytes_done in the shared
            // state so the aggregate emitter sees fresh numbers.
            let res = stream_one_file(
                &*host_c, &conn_c, &bucket_c, &path, &local_path,
                cancel_c.clone(), states_c.clone(), i,
            ).await;

            drop(permit);

            let mut s = states_c.lock().await;
            let (line, op_status, op_detail) = match &res {
                Ok(()) => {
                    s[i].status = "done";
                    (
                        format!("[{:>3}/{}] done:  {} ({})",
                            i + 1, n, s[i].basename, human_bytes(s[i].bytes_total)),
                        "completed",
                        Some(human_bytes(s[i].bytes_total)),
                    )
                }
                Err(CloudError::Cancelled) => {
                    s[i].status = "cancelled";
                    (
                        format!("[{:>3}/{}] cancelled: {}", i + 1, n, s[i].basename),
                        "skipped",
                        Some("cancelled".to_string()),
                    )
                }
                Err(e) => {
                    s[i].status = "failed";
                    s[i].error  = Some(e.to_string());
                    (
                        format!("[{:>3}/{}] FAILED: {} — {}", i + 1, n, s[i].basename, e),
                        "error",
                        Some(e.to_string()),
                    )
                }
            };
            host_c.job_append_output(&job_c, line);
            host_c.emit_event("arbor://plugin-operation-update", serde_json::json!({
                "id":     &op_c,
                "plugin": "cloud-storage",
                "kind":   "update_step",
                "step":   &step_key,
                "status": op_status,
                "detail": op_detail,
            }));
        });
        handles.push(h);
    }

    // Wait for all sub-downloads to finish (or fail).
    for h in handles {
        let _ = h.await;
    }

    // Stop the progress emitter; do one final synchronous emit so the UI
    // shows the terminal per-file states before the done hook lands.
    prog_done.store(true, Ordering::Relaxed);
    let _ = progress_task.await;
    emit_aggregate_progress(&*host, &stream_id, &op_label, &states).await;

    // Build the final done payload.
    let (ok, local_paths, error) = {
        let s = states.lock().await;
        let cancelled = cancel.load(Ordering::Relaxed);
        let any_fail  = s.iter().any(|f| f.status == "failed");
        let ok        = !cancelled && !any_fail;
        let locals: Vec<String> = s.iter().map(|f| f.local_path.clone()).collect();
        let err = if cancelled {
            Some("cancelled".to_string())
        } else if any_fail {
            // Concatenate per-file errors so the chunk handler sees what failed.
            let msgs: Vec<String> = s.iter()
                .filter(|f| f.status == "failed")
                .filter_map(|f| f.error.clone())
                .collect();
            Some(msgs.join("; "))
        } else {
            None
        };
        (ok, locals, err)
    };

    let done_payload = serde_json::json!({
        "stream_id":   stream_id,
        "ok":          ok,
        "error":       error,
        "local_paths": local_paths,
    });
    // Tauri channel for the host modal.
    host.emit_event(EVT_MANY_DONE, done_payload.clone());
    // Hook for the plugin orchestrator (the chunk-handler trigger).
    fire_plugin_hook(&*host, HOOK_MANY_DONE, done_payload);

    // Close the OperationsOverlay card with a per-status summary — unless
    // `keep_open` is set (chunk-merge flow), in which case the merge phase
    // takes over and `report_done` will close the card.
    //
    // Edge case: if the download phase ALREADY failed/cancelled, we close
    // immediately even with `keep_open=true` because there's nothing for
    // the merge phase to operate on. The pending-ops entry is wiped so
    // `report_done` (if called anyway) becomes a no-op.
    let download_failed = !ok;
    if !keep_open || download_failed {
        let s = states.lock().await;
        let done_n      = s.iter().filter(|f| f.status == "done").count();
        let failed_n    = s.iter().filter(|f| f.status == "failed").count();
        let cancelled_n = s.iter().filter(|f| f.status == "cancelled").count();
        let total_bytes: u64 = s.iter().filter(|f| f.status == "done")
            .map(|f| f.bytes_total).sum();
        let summary = if cancelled_n == n {
            "Cancelled before any file finished".to_string()
        } else {
            let mut parts = vec![format!("{done_n}/{n} done")];
            if failed_n    > 0 { parts.push(format!("{failed_n} failed")); }
            if cancelled_n > 0 { parts.push(format!("{cancelled_n} cancelled")); }
            parts.push(format!("{} total", human_bytes(total_bytes)));
            parts.join(" · ")
        };
        let error_msg = if !ok {
            error.clone().or_else(|| Some("download failed".to_string()))
        } else { None };
        host.emit_event("arbor://plugin-operation-finish", serde_json::json!({
            "id":      &op_id,
            "plugin":  "cloud-storage",
            "summary": summary,
            "error":   error_msg,
        }));

        let cancelled_flag = cancel.load(Ordering::Relaxed);
        let final_err = if ok {
            None
        } else if cancelled_flag {
            Some("cancelled".to_string())
        } else {
            error.clone().or_else(|| Some("unknown error".into()))
        };
        let status = if ok {
            CloudJobStatus::Completed { exit_code: 0 }
        } else if cancelled_flag {
            CloudJobStatus::Cancelled
        } else {
            CloudJobStatus::Failed {
                error: final_err.clone().unwrap_or_else(|| "unknown error".into()),
            }
        };
        host.job_set_status(&job_id, status);
        // Mirror the single-job `finalize_job` so JobsOverlay flips this
        // aggregate from running → terminal state.
        host.emit_event("arbor://job-done", serde_json::json!({
            "job_id":    job_id,
            "success":   ok,
            "exit_code": if ok { 0 } else { -1 },
            "cancelled": cancelled_flag,
            "error":     final_err,
        }));
        if let Ok(mut map) = host.cancellations().lock() {
            map.remove(&job_id);
            map.remove(&stream_id);
        };
        if let Ok(mut map) = host.pending_ops().lock() {
            map.remove(&stream_id);
        };
    }
    // When `keep_open` and the download phase succeeded, we deliberately
    // leave the cancellations map populated so the merge phase can still
    // be cancelled via `arbor.cloud.cancel`. `report_done` does the final
    // cleanup.
}

/// Download one object, mutating the shared state's `bytes_done` / `bytes_total`
/// so the aggregate emitter sees fresh values without a separate channel.
async fn stream_one_file(
    host:       &dyn CloudHost,
    conn:       &CloudConnection,
    bucket:     &str,
    remote:     &str,
    local:      &str,
    cancel:     Arc<AtomicBool>,
    states:     Arc<tokio::sync::Mutex<Vec<ManyFileState>>>,
    index:      usize,
) -> Result<()> {
    let _ = host; // reserved for per-file events later if we add them
    let op = build(conn, bucket).await?;
    let meta = op.stat(remote).await.map_err(map_op_err)?;
    let total = if meta.is_file() { meta.content_length() } else { 0 };
    {
        let mut s = states.lock().await;
        s[index].bytes_total = total;
    }

    let local_path = std::path::Path::new(local);
    if let Some(parent) = local_path.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", parent.display())))?;
        }
    }

    let mut reader = op.reader(remote).await.map_err(map_op_err)?
        .into_bytes_stream(0..)
        .await.map_err(map_op_err)?;
    let mut file = tokio::fs::File::create(local_path).await
        .map_err(|e| CloudError::Other(format!("create {}: {e}", local_path.display())))?;

    while let Some(chunk) = reader.next().await {
        if cancel.load(Ordering::Relaxed) { return Err(CloudError::Cancelled); }
        let bytes = chunk.map_err(|e| CloudError::Other(format!("opendal read: {e}")))?;
        file.write_all(&bytes).await
            .map_err(|e| CloudError::Other(format!("write {}: {e}", local_path.display())))?;
        let mut s = states.lock().await;
        s[index].bytes_done += bytes.len() as u64;
    }
    file.flush().await
        .map_err(|e| CloudError::Other(format!("flush {}: {e}", local_path.display())))?;
    Ok(())
}

async fn emit_aggregate_progress(
    host:      &dyn CloudHost,
    stream_id: &str,
    op_label:  &str,
    states:    &Arc<tokio::sync::Mutex<Vec<ManyFileState>>>,
) {
    let snapshot = states.lock().await.clone();
    let files_total = snapshot.len() as u64;
    let files_done  = snapshot.iter().filter(|f| f.status == "done").count() as u64;
    let bytes_done: u64  = snapshot.iter().map(|f| f.bytes_done).sum();
    let bytes_total: u64 = snapshot.iter().map(|f| f.bytes_total).sum();

    // Progress goes straight to the Tauri channel — modal-only, no Lua
    // subscriber needs the high-frequency stream.
    host.emit_event(EVT_MANY_PROGRESS, serde_json::json!({
        "stream_id": stream_id,
        "op_label":  op_label,
        "phase":     "download",
        "files":     snapshot,
        "aggregate": {
            "files_done":  files_done,
            "files_total": files_total,
            "bytes_done":  bytes_done,
            "bytes_total": bytes_total,
        },
    }));
}

// ── implementation ─────────────────────────────────────────────────────────

async fn run_download(
    host: &dyn CloudHost, job_id: &str,
    conn: &CloudConnection, bucket: &str, remote: &str, local: &Path,
    cancel: Arc<AtomicBool>,
) -> Result<()> {
    let op = build(conn, bucket).await?;
    let meta = op.stat(remote).await.map_err(map_op_err)?;
    let total = if meta.is_file() { meta.content_length() } else { 0 };

    if let Some(parent) = local.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", parent.display())))?;
        }
    }

    let mut reader = op.reader(remote).await.map_err(map_op_err)?
        .into_bytes_stream(0..)
        .await.map_err(map_op_err)?;
    let mut file = tokio::fs::File::create(local).await
        .map_err(|e| CloudError::Other(format!("create {}: {e}", local.display())))?;

    let mut ticker = ProgressTicker::new(total);
    while let Some(chunk) = reader.next().await {
        if cancel.load(Ordering::Relaxed) { return Err(CloudError::Cancelled); }
        let bytes = chunk.map_err(|e| CloudError::Other(format!("opendal read: {e}")))?;
        file.write_all(&bytes).await
            .map_err(|e| CloudError::Other(format!("write {}: {e}", local.display())))?;
        ticker.advance(bytes.len() as u64);
        ticker.maybe_emit(host, job_id, &conn.config_id, "download", bucket, remote);
    }
    file.flush().await
        .map_err(|e| CloudError::Other(format!("flush {}: {e}", local.display())))?;
    ticker.final_emit(host, job_id, &conn.config_id, "download", bucket, remote);
    Ok(())
}

async fn run_upload(
    host: &dyn CloudHost, job_id: &str,
    conn: &CloudConnection, bucket: &str, remote: &str, local: &Path,
    overwrite: bool, cancel: Arc<AtomicBool>,
) -> Result<()> {
    let op = build(conn, bucket).await?;

    if !overwrite {
        // Cheap existence check: opendal's `stat` returns NotFound when
        // the object isn't there, anything else means we found it (or hit
        // a real error worth surfacing).
        match op.stat(remote).await {
            Ok(_)  => return Err(CloudError::Other(format!(
                "destination already exists: {bucket}:{remote} (pass overwrite=true to replace)"
            ))),
            Err(e) if e.kind() == opendal::ErrorKind::NotFound => {}
            Err(e) => return Err(map_op_err(e)),
        }
    }

    let meta = tokio::fs::metadata(local).await
        .map_err(|e| CloudError::Other(format!("stat {}: {e}", local.display())))?;
    let total = meta.len();

    let mut file = tokio::fs::File::open(local).await
        .map_err(|e| CloudError::Other(format!("open {}: {e}", local.display())))?;

    let mut writer = op.writer(remote).await.map_err(map_op_err)?;
    let mut ticker = ProgressTicker::new(total);
    let mut buf = vec![0u8; CHUNK_SIZE];

    loop {
        if cancel.load(Ordering::Relaxed) {
            // Best-effort: abort the in-progress upload (opendal multipart).
            let _ = writer.abort().await;
            return Err(CloudError::Cancelled);
        }
        let n = file.read(&mut buf).await
            .map_err(|e| CloudError::Other(format!("read {}: {e}", local.display())))?;
        if n == 0 { break; }
        writer.write(buf[..n].to_vec()).await.map_err(map_op_err)?;
        ticker.advance(n as u64);
        ticker.maybe_emit(host, job_id, &conn.config_id, "upload", bucket, remote);
    }
    writer.close().await.map_err(map_op_err)?;
    ticker.final_emit(host, job_id, &conn.config_id, "upload", bucket, remote);
    Ok(())
}

async fn run_sync(
    host: &dyn CloudHost, job_id: &str,
    conn: &CloudConnection, bucket: &str, remote_prefix: &str, local: &Path,
    direction: SyncDir, delete: bool, cancel: Arc<AtomicBool>,
) -> Result<()> {
    match direction {
        SyncDir::Down => sync_down(host, job_id, conn, bucket, remote_prefix, local, delete, cancel).await,
        SyncDir::Up   => sync_up  (host, job_id, conn, bucket, remote_prefix, local, delete, cancel).await,
    }
}

async fn sync_down(
    host: &dyn CloudHost, job_id: &str,
    conn: &CloudConnection, bucket: &str, remote_prefix: &str, local: &Path,
    delete: bool, cancel: Arc<AtomicBool>,
) -> Result<()> {
    let op = build(conn, bucket).await?;
    let prefix = normalize_for_listing(remote_prefix);
    tokio::fs::create_dir_all(local).await
        .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", local.display())))?;

    // 1. Walk remote recursively, collect object keys + sizes.
    let entries = op.list_with(&prefix).recursive(true).await.map_err(map_op_err)?;
    let total_bytes: u64 = entries.iter()
        .filter(|e| !e.metadata().is_dir())
        .map(|e| e.metadata().content_length())
        .sum();

    append_job_line(host, job_id, &format!(
        "Found {} remote object(s), {} bytes",
        entries.iter().filter(|e| !e.metadata().is_dir()).count(),
        total_bytes
    ));

    // 2. Stream each file, tracking aggregate progress.
    let mut ticker = ProgressTicker::new(total_bytes);
    let mut seen_local: Vec<PathBuf> = Vec::new();

    for entry in &entries {
        if cancel.load(Ordering::Relaxed) { return Err(CloudError::Cancelled); }
        if entry.metadata().is_dir() { continue; }
        let rel = entry.path().strip_prefix(&prefix).unwrap_or(entry.path());
        let dst = local.join(rel);
        if let Some(parent) = dst.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| CloudError::Other(format!("mkdir {}: {e}", parent.display())))?;
        }

        let mut reader = op.reader(entry.path()).await.map_err(map_op_err)?
            .into_bytes_stream(0..).await.map_err(map_op_err)?;
        let mut f = tokio::fs::File::create(&dst).await
            .map_err(|e| CloudError::Other(format!("create {}: {e}", dst.display())))?;
        while let Some(chunk) = reader.next().await {
            if cancel.load(Ordering::Relaxed) { return Err(CloudError::Cancelled); }
            let bytes = chunk.map_err(|e| CloudError::Other(format!("opendal read: {e}")))?;
            f.write_all(&bytes).await
                .map_err(|e| CloudError::Other(format!("write {}: {e}", dst.display())))?;
            ticker.advance(bytes.len() as u64);
            ticker.maybe_emit(host, job_id, &conn.config_id, "sync", bucket, entry.path());
        }
        f.flush().await
            .map_err(|e| CloudError::Other(format!("flush {}: {e}", dst.display())))?;
        seen_local.push(dst);
    }

    // 3. delete=true → prune local files that don't exist remotely.
    if delete {
        prune_local_not_in(local, &seen_local).await?;
    }

    ticker.final_emit(host, job_id, &conn.config_id, "sync", bucket, remote_prefix);
    Ok(())
}

async fn sync_up(
    host: &dyn CloudHost, job_id: &str,
    conn: &CloudConnection, bucket: &str, remote_prefix: &str, local: &Path,
    delete: bool, cancel: Arc<AtomicBool>,
) -> Result<()> {
    let op = build(conn, bucket).await?;
    let prefix = normalize_for_listing(remote_prefix);

    // 1. Walk local directory recursively.
    let mut local_files: Vec<PathBuf> = Vec::new();
    walk_local(local, local, &mut local_files).await?;
    let total: u64 = futures_util::future::join_all(
        local_files.iter().map(|p| async move {
            tokio::fs::metadata(p).await.map(|m| m.len()).unwrap_or(0)
        })
    ).await.into_iter().sum();

    append_job_line(host, job_id, &format!(
        "Found {} local file(s), {total} bytes", local_files.len()
    ));

    let mut ticker = ProgressTicker::new(total);
    let mut seen_remote: Vec<String> = Vec::new();

    for src in &local_files {
        if cancel.load(Ordering::Relaxed) { return Err(CloudError::Cancelled); }
        let rel = src.strip_prefix(local).map_err(|_| CloudError::Other(
            "internal: local file fell outside the sync root".into()
        ))?;
        let rel_s = rel.to_string_lossy().replace('\\', "/");
        let remote = if prefix.is_empty() { rel_s.clone() } else { format!("{prefix}{rel_s}") };

        let mut f = tokio::fs::File::open(src).await
            .map_err(|e| CloudError::Other(format!("open {}: {e}", src.display())))?;
        let mut writer = op.writer(&remote).await.map_err(map_op_err)?;
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = writer.abort().await;
                return Err(CloudError::Cancelled);
            }
            let n = f.read(&mut buf).await
                .map_err(|e| CloudError::Other(format!("read {}: {e}", src.display())))?;
            if n == 0 { break; }
            writer.write(buf[..n].to_vec()).await.map_err(map_op_err)?;
            ticker.advance(n as u64);
            ticker.maybe_emit(host, job_id, &conn.config_id, "sync", bucket, &remote);
        }
        writer.close().await.map_err(map_op_err)?;
        seen_remote.push(remote);
    }

    // 2. delete=true → prune remote objects that don't exist locally.
    if delete {
        let entries = op.list_with(&prefix).recursive(true).await.map_err(map_op_err)?;
        for entry in entries {
            if entry.metadata().is_dir() { continue; }
            if !seen_remote.iter().any(|s| s == entry.path()) {
                let _ = op.delete(entry.path()).await;
            }
        }
    }

    ticker.final_emit(host, job_id, &conn.config_id, "sync", bucket, remote_prefix);
    Ok(())
}

// ── job shell ──────────────────────────────────────────────────────────────

fn spawn_job_shell(
    host:      &dyn CloudHost,
    config_id: &str,
    kind:      &str,
    label:     &str,
) -> Result<(String, Arc<AtomicBool>)> {
    let cancel = Arc::new(AtomicBool::new(false));

    let name = format!("{kind}: {label}");
    let command = format!("cloud:{kind}");
    let category = "Cloud Storage";

    let job_id = host.job_new_id();
    host.job_register(CloudJobInfo {
        id:          job_id.clone(),
        name:        name.clone(),
        plugin_name: "cloud-storage".into(),
        command:     command.clone(),
        started_at:  now_secs(),
        status:      CloudJobStatus::Running,
        category:    Some(category.into()),
        non_cancellable: false,
        hidden:      false,
        is_system:   false,
    });

    // Stash the cancel flag so the global cancel_job command can flip it.
    let mut map = host.cancellations().lock().map_err(|e|
        CloudError::Other(format!("cloud_cancellations poisoned: {e}"))
    )?;
    map.insert(job_id.clone(), cancel.clone());
    drop(map);

    // Match the payload shape the jobs.svelte.ts `arbor://job-started`
    // handler expects: `name`, `plugin_name`, `command`, `category`,
    // `hidden`. Without these the JobsOverlay renders the entry with a
    // blank name.
    host.emit_event("arbor://job-started", serde_json::json!({
        "job_id":      job_id,
        "name":        name,
        "plugin_name": "cloud-storage",
        "command":     command,
        "category":    category,
        "hidden":      false,
        "config_id":   config_id,
        "kind":        kind,
    }));
    Ok((job_id, cancel))
}

async fn finalize_job(
    host: &dyn CloudHost,
    job_id: &str,
    result: Result<()>,
    cancel: &Arc<AtomicBool>,
) {
    // Remove the cancel-flag entry now that the task is done.
    if let Ok(mut map) = host.cancellations().lock() {
        map.remove(job_id);
    }

    let cancelled = cancel.load(Ordering::Relaxed);
    let (status, ok, err) = match result {
        Ok(()) if cancelled => (CloudJobStatus::Cancelled, false, Some("cancelled".to_string())),
        Ok(())              => (CloudJobStatus::Completed { exit_code: 0 }, true, None),
        Err(CloudError::Cancelled) => (CloudJobStatus::Cancelled, false, Some("cancelled".to_string())),
        Err(_) if cancelled => (CloudJobStatus::Cancelled, false, Some("cancelled".to_string())),
        Err(e) => (CloudJobStatus::Failed { error: e.to_string() }, false, Some(e.to_string())),
    };
    let (exit_code, cancelled_flag) = match &status {
        CloudJobStatus::Completed { exit_code } => (*exit_code, false),
        CloudJobStatus::Cancelled               => (-1, true),
        CloudJobStatus::Failed   { .. }         => (-1, false),
        CloudJobStatus::Running                 => unreachable!(),
    };
    // Append a final summary line for the JobOutputPanel.
    let final_line = match &status {
        CloudJobStatus::Completed { .. } => "✓ done".to_string(),
        CloudJobStatus::Cancelled        => "✗ cancelled".to_string(),
        CloudJobStatus::Failed { error } => format!("✗ failed: {error}"),
        CloudJobStatus::Running          => unreachable!(),
    };
    host.job_append_output(job_id, final_line);
    host.job_set_status(job_id, status);

    // Same shape the standard subprocess `spawn_job` emits so the frontend's
    // `arbor://job-done` handler can flip the JobsOverlay entry from
    // "running" to the terminal state. Without this the cloud transfer's
    // JobsOverlay row stays on the spinner forever.
    host.emit_event("arbor://job-done", serde_json::json!({
        "job_id":    job_id,
        "success":   ok,
        "exit_code": exit_code,
        "cancelled": cancelled_flag,
        "error":     err.clone(),
    }));
    fire_plugin_hook(host, HOOK_JOB_DONE, serde_json::json!({
        "job_id": job_id,
        "ok":     ok,
        "error":  err,
    }));
}

// ── helpers ────────────────────────────────────────────────────────────────

fn normalize_for_listing(p: &str) -> String {
    if p.is_empty() || p == "/" { return String::new(); }
    if p.ends_with('/') { p.to_string() } else { format!("{p}/") }
}

async fn walk_local(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    // Iterative BFS to avoid the async-recursion + borrow-checker dance.
    let _ = root;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let mut rd = tokio::fs::read_dir(&d).await
            .map_err(|e| CloudError::Other(format!("readdir {}: {e}", d.display())))?;
        while let Some(entry) = rd.next_entry().await
            .map_err(|e| CloudError::Other(format!("readdir entry: {e}")))?
        {
            let p = entry.path();
            let ft = entry.file_type().await
                .map_err(|e| CloudError::Other(format!("stat {}: {e}", p.display())))?;
            if ft.is_dir() {
                stack.push(p);
            } else if ft.is_file() {
                out.push(p);
            }
        }
    }
    Ok(())
}

async fn prune_local_not_in(root: &Path, keep: &[PathBuf]) -> Result<()> {
    let mut all: Vec<PathBuf> = Vec::new();
    walk_local(root, root, &mut all).await?;
    for p in all {
        if !keep.iter().any(|k| k == &p) {
            let _ = tokio::fs::remove_file(&p).await;
        }
    }
    Ok(())
}

fn append_job_line(host: &dyn CloudHost, job_id: &str, line: &str) {
    host.job_append_output(job_id, line.to_string());
}

// ── progress ticker ────────────────────────────────────────────────────────

struct ProgressTicker {
    started:     Instant,
    last_emit:   Instant,
    last_bytes:  u64,
    bytes_done:  u64,
    bytes_total: u64,
}

impl ProgressTicker {
    fn new(bytes_total: u64) -> Self {
        let now = Instant::now();
        Self {
            started:     now,
            last_emit:   now,
            last_bytes:  0,
            bytes_done:  0,
            bytes_total,
        }
    }
    fn advance(&mut self, n: u64) { self.bytes_done += n; }

    fn maybe_emit(&mut self, host: &dyn CloudHost, job_id: &str, config_id: &str,
                  kind: &'static str, bucket: &str, path: &str)
    {
        let elapsed_ms = self.last_emit.elapsed().as_millis();
        if elapsed_ms < PROGRESS_TICK_MS { return; }
        self.emit(host, job_id, config_id, kind, bucket, path, elapsed_ms);
    }

    fn final_emit(&mut self, host: &dyn CloudHost, job_id: &str, config_id: &str,
                  kind: &'static str, bucket: &str, path: &str)
    {
        let elapsed_ms = self.last_emit.elapsed().as_millis().max(1);
        self.emit(host, job_id, config_id, kind, bucket, path, elapsed_ms);
    }

    fn emit(&mut self, host: &dyn CloudHost, job_id: &str, config_id: &str,
            kind: &'static str, bucket: &str, path: &str, elapsed_ms: u128)
    {
        let delta = self.bytes_done.saturating_sub(self.last_bytes);
        let speed_bps = if elapsed_ms == 0 { 0 } else {
            ((delta as u128 * 1000) / elapsed_ms) as u64
        };
        let eta = if speed_bps == 0 || self.bytes_total == 0 { None } else {
            let remaining = self.bytes_total.saturating_sub(self.bytes_done);
            Some(remaining / speed_bps.max(1))
        };
        let progress = CloudProgress {
            job_id:       job_id.to_string(),
            config_id:    config_id.to_string(),
            kind,
            bucket:       bucket.to_string(),
            path:         path.to_string(),
            bytes_done:   self.bytes_done,
            bytes_total:  self.bytes_total,
            speed_bps,
            eta_sec:      eta,
        };
        fire_plugin_hook(host, HOOK_PROGRESS,
            serde_json::to_value(&progress).unwrap_or(serde_json::Value::Null));

        // Append a human-readable line to the job output (used by the
        // generic JobOutputPanel — separate from the progress event).
        host.job_append_output(job_id, format!(
            "{} {} {}/{} ({}%) {}/s",
            kind_arrow(kind),
            short_path(path),
            human_bytes(self.bytes_done),
            if self.bytes_total > 0 { human_bytes(self.bytes_total) } else { "?".into() },
            if self.bytes_total > 0 { 100 * self.bytes_done / self.bytes_total } else { 0 },
            human_bytes(speed_bps),
        ));

        self.last_emit = Instant::now();
        self.last_bytes = self.bytes_done;
        let _ = self.started;  // kept for future "average speed" line
    }
}

fn kind_arrow(k: &str) -> &'static str {
    match k {
        "download" => "↓",
        "upload"   => "↑",
        "sync"     => "↔",
        _          => "•",
    }
}

fn short_path(p: &str) -> String {
    const MAX: usize = 56;
    if p.len() <= MAX { return p.to_string(); }
    let tail = &p[p.len() - (MAX - 1)..];
    format!("…{tail}")
}

fn human_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let nf = n as f64;
    if nf >= GB        { format!("{:.2} GB", nf / GB) }
    else if nf >= MB   { format!("{:.2} MB", nf / MB) }
    else if nf >= KB   { format!("{:.1} KB", nf / KB) }
    else               { format!("{n} B") }
}
