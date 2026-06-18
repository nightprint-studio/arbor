//! `diff` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run
//! inline; `#[corvus::handler]` self-registers it under its **own function
//! name**, so the command is reached generically through the router. Behavior
//! (locks held, errors, brief-lock-then-reopen shape) is byte-identical — only
//! the call path changed.
//!
//! The pure git work already lives in the reusable shell module
//! [`crate::git::diff`] (libgit2-based, no Tauri / no credentials), so these
//! handlers delegate to it directly — no `corvus-git` crate extraction needed.
//! The generic `rpc` command already wraps dispatch in `spawn_blocking`, so the
//! per-handler `tokio::task::spawn_blocking` of the old async commands is
//! dropped: each handler reopens the repo off the brief repo-lock and runs the
//! git work inline.
//!
//! No hooks fire in this domain.
//!
//! `get_workdir_diff_stream` is a deferred-emit handler: it returns a `job_id`
//! immediately and streams `arbor://diff-stream-*` events from a background
//! thread via the **event sink** (`Arc<dyn EventSink>` — [`AppState::event_sink`])
//! instead of an `AppHandle`. The background thread reaches the job registry
//! through a captured `Arc::clone(&state.jobs)`, never an `AppHandle`. Behavior
//! (topics, payloads, job entries) is byte-identical to the old inline command.
//!
//! `get_file_blame_streaming` was converted off `tauri::ipc::Channel` (the last
//! real `Channel` in the codebase) onto the streaming seam: it still returns the
//! assembled `Vec<BlameLine>` synchronously (it blocks until the walk finishes),
//! but emits **progress** ticks via a [`Stream`] on base `arbor://blame-stream`.
//! The FE supplies the `stream_id` and subscribes to it before invoking, so a
//! fast first tick can't outrun the listener. Progress ticks ride `-chunk`; no
//! terminal `done`/`error` is emitted (the invoke promise resolving with the
//! lines is the completion signal the FE awaits).

use std::collections::HashMap;
use std::sync::Arc;

use arbor_ipc::prelude::Stream;

use crate::error::AppError;
use crate::git::blame_incremental::BlameProgress;
use crate::git::diff::{BlameLine, DiffFile};
use crate::ipc::corvus;
use crate::jobs::{JobInfo, JobStatus};
use crate::AppState;

/// Frontend supplies `encoding_overrides` as `{ [path]: "windows-1252" }`.
/// `None` means "no overrides — auto-detect every file" (default behaviour).
type Overrides = Option<HashMap<String, String>>;

#[corvus::handler]
fn get_commit_diff(
    state: &AppState,
    tab_id: String,
    oid: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_diff(
        &repo, &oid, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_commit_diff_meta(
    state: &AppState,
    tab_id: String,
    oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<crate::git::diff::DiffFile>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_diff_meta(&repo, &oid, diff_algo.as_deref())
}

#[corvus::handler]
fn get_commit_file_diff(
    state: &AppState,
    tab_id: String,
    oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<crate::git::diff::DiffFile, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commit_file_diff(
        &repo, &oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_commits_range_diff_meta(
    state: &AppState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<crate::git::diff::DiffFile>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commits_range_diff_meta(&repo, &base_oid, &target_oid, diff_algo.as_deref())
}

#[corvus::handler]
fn get_commits_range_file_diff(
    state: &AppState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<crate::git::diff::DiffFile, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_commits_range_file_diff(
        &repo, &base_oid, &target_oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_workdir_diff(
    state: &AppState,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_workdir_diff(
        &repo, staged, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_file_at_commit(
    state: &AppState,
    tab_id: String,
    oid: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<String, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_file_at_commit(&repo, &oid, &path, encoding_override.as_deref())
}

#[corvus::handler]
fn get_branch_diff(
    state: &AppState,
    tab_id: String,
    from_ref: String,
    to_ref: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, AppError> {
    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_branch_diff(
        &repo, &from_ref, &to_ref, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
}

#[corvus::handler]
fn get_file_blame(
    state: &AppState,
    tab_id: String,
    path: String,
) -> Result<Vec<BlameLine>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    let repo = git2::Repository::open(&repo_path)?;
    crate::git::diff::get_file_blame(&repo, &path)
}

/// Streaming blame: drives a determinate progress bar via `git blame
/// --incremental` while the history walk runs, **returning the assembled lines
/// synchronously** when it completes (the generic `rpc` command already wraps
/// dispatch in `spawn_blocking`, so blocking here is fine).
///
/// Progress is delivered out-of-band on the streaming seam: the FE supplies a
/// `stream_id` and subscribes to `arbor://blame-stream-chunk` before invoking;
/// every throttled `BlameProgress` tick rides one `-chunk` event. There is no
/// terminal `-done`/`-error` event — the invoke resolving with the `Vec`
/// (or rejecting) is the completion signal. The producer-side throttling
/// (~1% steps) is unchanged: it lives in `run_incremental_blame`.
///
/// Falls back to the libgit2 path (no progress ticks) when no `git` binary is
/// available, so the modal still works on a machine without git on PATH.
#[corvus::handler]
fn get_file_blame_streaming(
    state:     &AppState,
    tab_id:    String,
    path:      String,
    stream_id: String,
) -> Result<Vec<BlameLine>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };

    let has_git = crate::git_cli::snapshot().path.is_some();

    if !has_git {
        // No progress channel on the libgit2 fallback (it's all-or-nothing).
        let repo = git2::Repository::open(&repo_path)?;
        return crate::git::diff::get_file_blame(&repo, &path);
    }

    // Back the producer's `Fn(BlameProgress)` callback with `stream.chunk(...)`.
    // Pure egress: no JobInfo, no cancellation registry — just progress events.
    let stream = state.event_sink().map(|sink| {
        Stream::new(sink, "arbor://blame-stream", stream_id)
    });

    crate::git::blame_incremental::run_incremental_blame(
        std::path::Path::new(&repo_path),
        &path,
        |p: BlameProgress| {
            if let Some(s) = &stream {
                // `BlameProgress` serializes to a JSON object (camelCase) which
                // rides under the `{ stream_id, seq }` envelope as the chunk.
                if let Ok(v) = serde_json::to_value(&p) {
                    s.chunk(v);
                }
            }
        },
    )
}

/// Stream workdir/index diff to the frontend file-by-file via the standardized
/// [`Stream`] seam on base `arbor://diff-stream` (`stream_id == job_id`).
///
/// Phase 1 (synchronous, fast): compute the list of files + their delta status
/// without parsing hunks, emit `arbor://diff-stream-started` with the count and
/// metadata list so the UI can render a spinner + placeholder rows immediately.
///
/// Phase 2 (background thread): re-open the repo off the IPC thread, rebuild
/// the diff, and parse each file's hunks one at a time, emitting
/// `arbor://diff-stream-chunk` per file.  Emits `arbor://diff-stream-done` when
/// all files are parsed (or on error via `arbor://diff-stream-error`). Every
/// payload carries the `{ stream_id, seq }` envelope.
///
/// Returns a `job_id` (== `stream_id`) the frontend uses to correlate events
/// for the current request and to show a job entry in the statusbar. The
/// background thread emits through the captured event sink and reaches the job
/// registry via a cloned `Arc` (no `AppHandle`).
#[corvus::handler]
fn get_workdir_diff_stream(
    state: &AppState,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<String, AppError> {
    let sink = state
        .event_sink()
        .ok_or_else(|| AppError::Other("event sink unavailable".into()))?;
    let jobs = Arc::clone(&state.jobs);

    let ctx = context_lines.unwrap_or_else(|| {
        state.lock_config().map(|c| c.diff.context_lines).unwrap_or(3)
    });

    // Fast phase 1: compute metadata list on the IPC thread under the repo lock.
    let (repo_path, meta) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let repo_path = repo.path.clone();
        let diff = crate::git::diff::build_workdir_diff(repo.inner(), staged, ctx, diff_algo.as_deref())?;
        let meta = crate::git::diff::parse_diff_meta(&diff);
        (repo_path, meta)
    };

    // Register a short-lived internal job so the UI can optionally surface
    // the parsing activity (non-cancellable — parsing is cheap and in-process).
    // Flagged as `is_system` so it is auto-purged from the Jobs overlay a few
    // seconds after completion.
    let job_id = {
        let mut jobs = jobs.lock().map_err(|_| AppError::Other("jobs mutex poisoned".into()))?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id: id.clone(),
            name: format!("Loading diff ({} files)", meta.len()),
            plugin_name: "arbor".to_string(),
            command: format!("diff-stream:{tab_id}"),
            started_at: crate::jobs::JobRegistry::now_secs(),
            status: JobStatus::Running,
            category: Some("System".to_string()),
            non_cancellable: true,
            is_system: true,
            finished_at: None,
            hidden: false,
            target: None,
        });
        id
    };

    // `stream_id == job_id`: one identity addresses the Jobs entry and the
    // stream quartet.
    let stream = Stream::new(Arc::clone(&sink), "arbor://diff-stream", job_id.clone());

    // Emit started event synchronously so the frontend UI updates immediately.
    stream.started(serde_json::json!({
        "tab_id":      &tab_id,
        "staged":      staged,
        "total_files": meta.len(),
        "files":       &meta,
    }));

    // Short-circuit for empty diffs — no need to spawn a thread.
    if meta.is_empty() {
        stream.done(serde_json::json!({ "tab_id": &tab_id }));
        if let Ok(mut jobs) = jobs.lock() {
            jobs.set_status(&job_id, JobStatus::Completed { exit_code: 0 });
        };
        return Ok(job_id);
    }

    // Phase 2: spawn a background thread that re-opens the repo and parses
    // each delta individually.  We deliberately do NOT hold the state mutex
    // during this phase — other IPC calls (status refresh, graph load, …)
    // can proceed concurrently.
    let tab_id_thread = tab_id.clone();
    let algo_thread   = diff_algo.clone();
    let stream_bg     = stream.clone();
    let jobs_bg       = Arc::clone(&jobs);
    let job_id_thread = job_id.clone();

    let spawn_result = std::thread::Builder::new()
        .name(format!("arbor-diff-stream-{job_id}"))
        .spawn(move || {
            let run = || -> Result<(), AppError> {
                let repo = git2::Repository::open(&repo_path)
                    .map_err(AppError::from)?;
                let diff = crate::git::diff::build_workdir_diff(
                    &repo,
                    staged,
                    ctx,
                    algo_thread.as_deref(),
                )?;
                let total = diff.deltas().count();
                for i in 0..total {
                    let file = crate::git::diff::parse_diff_one(
                        &repo, &diff, i, encoding_overrides.as_ref(),
                    )?;
                    stream_bg.chunk(serde_json::json!({
                        "tab_id": &tab_id_thread,
                        "index":  i,
                        "total":  total,
                        "file":   file,
                    }));
                }
                Ok(())
            };

            match run() {
                Ok(()) => {
                    stream_bg.done(serde_json::json!({ "tab_id": &tab_id_thread }));
                    if let Ok(mut jobs) = jobs_bg.lock() {
                        jobs.set_status(&job_id_thread, JobStatus::Completed { exit_code: 0 });
                    };
                }
                Err(e) => {
                    let err = e.to_string();
                    stream_bg.error(&err);
                    if let Ok(mut jobs) = jobs_bg.lock() {
                        jobs.set_status(&job_id_thread, JobStatus::Failed { error: err });
                    };
                }
            }
        });

    if let Err(e) = spawn_result {
        // Fail gracefully and mark the job accordingly so the UI can recover.
        let err = format!("failed to spawn diff-stream thread: {e}");
        stream.error(&err);
        if let Ok(mut jobs) = jobs.lock() {
            jobs.set_status(&job_id, JobStatus::Failed { error: err.clone() });
        };
        return Err(AppError::Other(err));
    }

    Ok(job_id)
}
