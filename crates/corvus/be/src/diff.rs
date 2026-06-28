//! `diff` domain — commit/workdir/branch diffs + blame, served **out-of-process**.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::diff`), but the context is [`CorvusState`] and the
//! pure git work is the shared `corvus-git` `diff` module (libgit2 + the `git`
//! CLI for incremental blame) — so results and error wire strings are identical.
//! The per-call `context_lines` falls back to the shell-pushed `diff.context_lines`
//! exactly as in-process (via [`diff_context_lines`]). **No hooks fire** in this
//! domain.
//!
//! Two handlers stream instead of returning their whole payload, riding the
//! transport-agnostic [`Stream`]/`EventSink` seam (each emit becomes an `Event`
//! frame the shell re-emits to the FE — byte-identical topics/payloads):
//! - `get_file_blame_streaming` — pure egress: blocks until the walk finishes and
//!   returns the assembled `Vec<BlameLine>`, emitting `arbor://blame-stream-chunk`
//!   progress ticks meanwhile. Falls back to the libgit2 blame (no ticks) when no
//!   `git` program was pushed.
//! - `get_workdir_diff_stream` — returns a `job_id` (== `stream_id`) immediately,
//!   then a background thread parses each file and emits `arbor://diff-stream-*`.
//!   The Jobs-overlay entry is driven over the reverse channel via [`JobHandle`]
//!   (ADR-3); like the in-process copy it emits **no** `arbor://job-*` lifecycle
//!   events — only the diff-stream quartet — so the FE contract is unchanged.

use std::collections::HashMap;
use std::path::Path;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::Stream;
use corvus_core::prelude::CorvusState;
use corvus_git::diff::{BlameLine, DiffFile};
use git2::Repository;

use crate::jobs::JobHandle;
use crate::repo::{diff_context_lines, open, repo_path};

/// Frontend supplies `encoding_overrides` as `{ [path]: "windows-1252" }`.
/// `None` means "no overrides — auto-detect every file" (default behaviour).
type Overrides = Option<HashMap<String, String>>;

// ── Single-shot reads ────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn get_commit_diff(
    state: &CorvusState,
    tab_id: String,
    oid: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, String> {
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_commit_diff(
        &repo, &oid, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_commit_diff_meta(
    state: &CorvusState,
    tab_id: String,
    oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<DiffFile>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_commit_diff_meta(&repo, &oid, diff_algo.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_commit_file_diff(
    state: &CorvusState,
    tab_id: String,
    oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<DiffFile, String> {
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_commit_file_diff(
        &repo, &oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_commits_range_diff_meta(
    state: &CorvusState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<DiffFile>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_commits_range_diff_meta(
        &repo, &base_oid, &target_oid, diff_algo.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_commits_range_file_diff(
    state: &CorvusState,
    tab_id: String,
    base_oid: String,
    target_oid: String,
    path: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<DiffFile, String> {
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_commits_range_file_diff(
        &repo, &base_oid, &target_oid, &path, ctx, diff_algo.as_deref(),
        encoding_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_workdir_diff(
    state: &CorvusState,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, String> {
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_workdir_diff(
        &repo, staged, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_file_at_commit(
    state: &CorvusState,
    tab_id: String,
    oid: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<String, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_file_at_commit(&repo, &oid, &path, encoding_override.as_deref())
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_branch_diff(
    state: &CorvusState,
    tab_id: String,
    from_ref: String,
    to_ref: String,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<Vec<DiffFile>, String> {
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_branch_diff(
        &repo, &from_ref, &to_ref, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
    )
    .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn get_file_blame(state: &CorvusState, tab_id: String, path: String) -> Result<Vec<BlameLine>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::diff::get_file_blame(&repo, &path).map_err(|e| e.to_string())
}

// ── Streaming blame (returns lines synchronously, ticks via Stream) ───────────

/// Streaming blame: drives a determinate progress bar via `git blame
/// --incremental` while the history walk runs, returning the assembled lines
/// synchronously when it completes (the serve loop dispatches each request on its
/// own worker thread, so blocking here is fine).
///
/// Progress rides the streaming seam: the FE supplies a `stream_id` and
/// subscribes to `arbor://blame-stream-chunk` before invoking, so a fast first
/// tick can't outrun the listener. No terminal `-done`/`-error` — the call
/// resolving with the `Vec` (or rejecting) is the completion signal. Falls back
/// to the libgit2 blame (no ticks) when the shell pushed no `git` program.
#[arbor_rpc::handler]
fn get_file_blame_streaming(
    state: &CorvusState,
    tab_id: String,
    path: String,
    stream_id: String,
) -> Result<Vec<BlameLine>, String> {
    let repo_path = repo_path(state, &tab_id)?;

    // corvus-be self-detects its git program; its absence (no git binary found)
    // falls back to the libgit2 blame path instead of the streaming CLI one.
    if corvus_git_cli::snapshot().path.is_none() {
        let repo = Repository::open(&repo_path).map_err(|e| e.to_string())?;
        return corvus_git::diff::get_file_blame(&repo, &path).map_err(|e| e.to_string());
    }

    // Pure egress: no JobInfo, no cancellation registry — just progress ticks.
    let stream = Stream::new(state.event_sink(), "arbor://blame-stream", stream_id);

    corvus_git::diff::run_incremental_blame(
        &crate::repo::git(state),
        Path::new(&repo_path),
        &path,
        |p| {
            // `BlameProgress` serializes to a camelCase JSON object that rides
            // under the `{ stream_id, seq }` envelope as the chunk payload.
            if let Ok(v) = serde_json::to_value(&p) {
                stream.chunk(v);
            }
        },
    )
    .map_err(|e| e.to_string())
}

// ── Streaming workdir diff (id now, files via Stream from a worker thread) ────

/// Stream workdir/index diff to the frontend file-by-file via the standardized
/// [`Stream`] seam on base `arbor://diff-stream` (`stream_id == job_id`).
///
/// Phase 1 (synchronous, fast): compute the file list + delta status without
/// parsing hunks and emit `arbor://diff-stream-started` so the UI can render
/// placeholders immediately. Phase 2 (background thread): re-open the repo, parse
/// each file's hunks, emit one `arbor://diff-stream-chunk` per file, then `-done`
/// (or `-error`). The Jobs-overlay entry is registered + closed over the reverse
/// channel via [`JobHandle`]; no `arbor://job-*` lifecycle events are emitted
/// (matching the in-process copy). Returns the `job_id`.
#[arbor_rpc::handler]
fn get_workdir_diff_stream(
    state: &CorvusState,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<String, String> {
    let host = state
        .host_caller()
        .ok_or_else(|| "get_workdir_diff_stream: no reverse channel for jobs".to_string())?;
    let sink = state.event_sink();
    let ctx = context_lines.unwrap_or_else(|| diff_context_lines(state));

    // Fast phase 1: compute metadata on this worker thread. The background phase
    // re-opens by the pushed path (the repo root), not `Repository::path()` (the
    // `.git` dir), matching the in-process copy.
    let repo_path = repo_path(state, &tab_id)?;
    let meta = {
        let repo = open(state, &tab_id)?;
        let diff = corvus_git::diff::build_workdir_diff(&repo, staged, ctx, diff_algo.as_deref())
            .map_err(|e| e.to_string())?;
        corvus_git::diff::parse_diff_meta(&diff)
    };

    // Register a short-lived system job in the shell registry (non-cancellable —
    // parsing is cheap and in-process; `is_system` auto-purges it from the
    // overlay). The shell mints the id; `stream_id == job_id`.
    let job = JobHandle::register(
        host,
        JobSpec {
            name: format!("Loading diff ({} files)", meta.len()),
            plugin_name: "arbor".to_string(),
            command: format!("diff-stream:{tab_id}"),
            category: Some("System".to_string()),
            non_cancellable: true,
            hidden: false,
            is_system: true,
            target: None,
        },
    )?;
    let job_id = job.id.clone();
    // A spare handle so the thread-spawn-error path below can still fail the job
    // (the primary handle is moved into the worker closure).
    let job_spawn_guard = job.clone_handle();

    let stream = Stream::new(sink, "arbor://diff-stream", job_id.clone());
    stream.started(serde_json::json!({
        "tab_id":      &tab_id,
        "staged":      staged,
        "total_files": meta.len(),
        "files":       &meta,
    }));

    // Short-circuit for empty diffs — no thread needed.
    if meta.is_empty() {
        stream.done(serde_json::json!({ "tab_id": &tab_id }));
        job.set_status(JobStatus::Completed { exit_code: 0 });
        return Ok(job_id);
    }

    // Phase 2: parse each delta off-thread so other requests proceed concurrently.
    let tab_id_thread = tab_id.clone();
    let algo_thread = diff_algo.clone();
    let stream_bg = stream.clone();

    let spawn_result = std::thread::Builder::new()
        .name(format!("corvus-diff-stream-{job_id}"))
        .spawn(move || {
            let run = || -> Result<(), String> {
                let repo = Repository::open(&repo_path).map_err(|e| e.to_string())?;
                let diff =
                    corvus_git::diff::build_workdir_diff(&repo, staged, ctx, algo_thread.as_deref())
                        .map_err(|e| e.to_string())?;
                let total = diff.deltas().count();
                for i in 0..total {
                    let file = corvus_git::diff::parse_diff_one(
                        &repo, &diff, i, encoding_overrides.as_ref(),
                    )
                    .map_err(|e| e.to_string())?;
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
                    job.set_status(JobStatus::Completed { exit_code: 0 });
                }
                Err(err) => {
                    stream_bg.error(&err);
                    job.set_status(JobStatus::Failed { error: err });
                }
            }
        });

    if let Err(e) = spawn_result {
        let err = format!("failed to spawn diff-stream thread: {e}");
        stream.error(&err);
        job_spawn_guard.set_status(JobStatus::Failed { error: err.clone() });
        return Err(err);
    }

    Ok(job_id)
}
