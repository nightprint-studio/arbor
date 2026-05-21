use std::collections::HashMap;

use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::AppError;
use crate::git::diff::{BlameLine, DiffFile};
use crate::jobs::{JobInfo, JobStatus};
use crate::AppState;

/// Frontend supplies `encoding_overrides` as `{ [path]: "windows-1252" }`.
/// `None` means "no overrides — auto-detect every file" (default behaviour).
type Overrides = Option<HashMap<String, String>>;

#[tauri::command]
pub async fn get_commit_diff(
    state: State<'_, AppState>,
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
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_commit_diff(
            &repo, &oid, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
        )
    })
    .await
    .map_err(|e| AppError::Other(format!("get_commit_diff task panicked: {e}")))?
}

/// Metadata-only commit diff — returns the file list (path, status, +/- stats)
/// without parsing any hunks. Paired with `get_commit_file_diff` for the lazy
/// commit-detail viewer: clicking a commit shows the list instantly, then each
/// file's hunks are fetched on demand only when the user selects it. This
/// keeps the click responsive even on large commits and when the user has
/// `full_file = true` (which would otherwise force libgit2 to walk every byte
/// of every modified file before the IPC call returned).
#[tauri::command]
pub async fn get_commit_diff_meta(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
    diff_algo: Option<String>,
) -> Result<Vec<crate::git::diff::DiffFile>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_commit_diff_meta(&repo, &oid, diff_algo.as_deref())
    })
    .await
    .map_err(|e| AppError::Other(format!("get_commit_diff_meta task panicked: {e}")))?
}

/// Per-file commit diff — parses hunks for a single file inside the given
/// commit. Use after `get_commit_diff_meta` populated the file list to fill in
/// hunks on demand.
#[tauri::command]
pub async fn get_commit_file_diff(
    state: State<'_, AppState>,
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
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_commit_file_diff(
            &repo, &oid, &path, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
        )
    })
    .await
    .map_err(|e| AppError::Other(format!("get_commit_file_diff task panicked: {e}")))?
}

#[tauri::command]
pub async fn get_workdir_diff(
    state: State<'_, AppState>,
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
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_workdir_diff(
            &repo, staged, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
        )
    })
    .await
    .map_err(|e| AppError::Other(format!("get_workdir_diff task panicked: {e}")))?
}

/// Stream workdir/index diff to the frontend file-by-file.
///
/// Phase 1 (synchronous, fast): compute the list of files + their delta status
/// without parsing hunks, emit `arbor://diff-stream-started` with the count and
/// metadata list so the UI can render a spinner + placeholder rows immediately.
///
/// Phase 2 (background thread): re-open the repo off the IPC thread, rebuild
/// the diff, and parse each file's hunks one at a time, emitting
/// `arbor://diff-stream-file` per file.  Emits `arbor://diff-stream-done` when
/// all files are parsed (or on error via `arbor://diff-stream-error`).
///
/// Returns a `job_id` the frontend can use to correlate events for the current
/// request and to show a job entry in the statusbar.
#[tauri::command]
pub fn get_workdir_diff_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    staged: bool,
    context_lines: Option<u32>,
    diff_algo: Option<String>,
    encoding_overrides: Overrides,
) -> Result<String, AppError> {
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
        let mut jobs = state.jobs.lock().map_err(|_| AppError::Other("jobs mutex poisoned".into()))?;
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
        });
        id
    };

    // Emit started event synchronously so the frontend UI updates immediately.
    let _ = app.emit("arbor://diff-stream-started", serde_json::json!({
        "job_id":      &job_id,
        "tab_id":      &tab_id,
        "staged":      staged,
        "total_files": meta.len(),
        "files":       &meta,
    }));

    // Short-circuit for empty diffs — no need to spawn a thread.
    if meta.is_empty() {
        let _ = app.emit("arbor://diff-stream-done", serde_json::json!({
            "job_id": &job_id,
            "tab_id": &tab_id,
        }));
        if let Ok(mut jobs) = state.jobs.lock() {
            jobs.set_status(&job_id, JobStatus::Completed { exit_code: 0 });
        }
        return Ok(job_id);
    }

    // Phase 2: spawn a background thread that re-opens the repo and parses
    // each delta individually.  We deliberately do NOT hold the state mutex
    // during this phase — other IPC calls (status refresh, graph load, …)
    // can proceed concurrently.
    let job_id_thread = job_id.clone();
    let tab_id_thread = tab_id.clone();
    let algo_thread   = diff_algo.clone();
    let app_thread    = app.clone();

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
                    let _ = app_thread.emit("arbor://diff-stream-file", serde_json::json!({
                        "job_id": &job_id_thread,
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
                    let _ = app_thread.emit("arbor://diff-stream-done", serde_json::json!({
                        "job_id": &job_id_thread,
                        "tab_id": &tab_id_thread,
                    }));
                    let state = app_thread.state::<AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        jobs.set_status(&job_id_thread, JobStatus::Completed { exit_code: 0 });
                    };
                }
                Err(e) => {
                    let err = e.to_string();
                    let _ = app_thread.emit("arbor://diff-stream-error", serde_json::json!({
                        "job_id": &job_id_thread,
                        "tab_id": &tab_id_thread,
                        "error":  err.clone(),
                    }));
                    let state = app_thread.state::<AppState>();
                    if let Ok(mut jobs) = state.jobs.lock() {
                        jobs.set_status(&job_id_thread, JobStatus::Failed { error: err });
                    };
                }
            }
        });

    if let Err(e) = spawn_result {
        // Fail gracefully and mark the job accordingly so the UI can recover.
        let err = format!("failed to spawn diff-stream thread: {e}");
        let _ = app.emit("arbor://diff-stream-error", serde_json::json!({
            "job_id": &job_id,
            "tab_id": &tab_id,
            "error":  err.clone(),
        }));
        if let Ok(mut jobs) = state.jobs.lock() {
            jobs.set_status(&job_id, JobStatus::Failed { error: err.clone() });
        }
        return Err(AppError::Other(err));
    }

    Ok(job_id)
}

#[tauri::command]
pub async fn get_file_at_commit(
    state: State<'_, AppState>,
    tab_id: String,
    oid: String,
    path: String,
    encoding_override: Option<String>,
) -> Result<String, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_file_at_commit(&repo, &oid, &path, encoding_override.as_deref())
    })
    .await
    .map_err(|e| AppError::Other(format!("get_file_at_commit task panicked: {e}")))?
}

#[tauri::command]
pub async fn get_branch_diff(
    state: State<'_, AppState>,
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
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_branch_diff(
            &repo, &from_ref, &to_ref, ctx, diff_algo.as_deref(), encoding_overrides.as_ref(),
        )
    })
    .await
    .map_err(|e| AppError::Other(format!("get_branch_diff task panicked: {e}")))?
}

#[tauri::command]
pub async fn get_file_blame(
    state: State<'_, AppState>,
    tab_id: String,
    path: String,
) -> Result<Vec<BlameLine>, AppError> {
    let repo_path = {
        let mut mgr = state.lock_repos()?;
        mgr.get(&tab_id)?.path.clone()
    };
    tokio::task::spawn_blocking(move || {
        let repo = git2::Repository::open(&repo_path)?;
        crate::git::diff::get_file_blame(&repo, &path)
    })
    .await
    .map_err(|e| AppError::Other(format!("get_file_blame task panicked: {e}")))?
}
