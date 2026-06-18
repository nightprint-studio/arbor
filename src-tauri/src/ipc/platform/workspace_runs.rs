//! `workspace` background runners — fetch-all / pull-all / tag-all.
//!
//! Split out of [`crate::ipc::platform::workspace`] (which kept the workspace
//! mutations + import commits) to keep each file focused. These three
//! `#[platform::handler]`s register a system [`Job`](crate::jobs::JobInfo),
//! spawn one background thread, and stream `arbor://job-*` / `arbor://workspace-*`
//! progress events through the backend [`EventSink`] (carried into the worker
//! thread alongside the shared `Arc<Mutex<JobRegistry>>` — no `AppHandle`). They
//! return immediately with the job id; the worker runs the per-repo git work
//! sequentially. Errors never abort the run — they're collected and reported in
//! the per-row events + the final summary.

use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::EventSink;

use crate::commands::workspace_commands::WorkspaceFetchStartResult;
use crate::error::AppError;
use crate::ipc::platform;
use crate::jobs::{JobInfo, JobRegistry, JobStatus};
use crate::AppState;

/// Emit through the backend sink when present (it always is once the backend is
/// wired; `None` only in a not-yet-initialised window, where dropping is safe).
fn sink_emit(sink: &Option<Arc<dyn EventSink>>, topic: &str, payload: serde_json::Value) {
    if let Some(s) = sink {
        s.emit(topic, payload);
    }
}

/// Append a line to the job's output buffer and mirror it to the Jobs overlay.
fn log_and_emit(
    sink: &Option<Arc<dyn EventSink>>,
    jobs: &Arc<Mutex<JobRegistry>>,
    job_id: &str,
    line: &str,
) {
    if let Ok(mut j) = jobs.lock() {
        j.append_output(job_id, line.to_string());
    }
    sink_emit(sink, "arbor://job-output", serde_json::json!({
        "job_id": job_id,
        "text":   line,
    }));
}

/// Register a system Job for a workspace-wide run and emit `arbor://job-started`.
/// Returns the new job id.
fn start_workspace_job(
    state: &AppState,
    job_name: &str,
    job_cmd: &str,
) -> Result<String, AppError> {
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            job_name.to_string(),
            plugin_name:     "arbor".into(),
            command:         job_cmd.to_string(),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("System".into()),
            non_cancellable: false,
            is_system:       true,
            finished_at:     None,
            hidden:          false,
            target:          None,
        });
        id
    };
    // Include every field the frontend reads out of the event — otherwise
    // `upsertJob` overwrites the registry row with `name = undefined`.
    state.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        job_name,
        "plugin_name": "arbor",
        "command":     job_cmd,
        "category":    "System",
    }));
    Ok(job_id)
}

/// Freeze the (repo_id, path, display_name) targets of a workspace's existing
/// repos under the locks, then release them before the (slow) run.
fn workspace_targets(
    state: &AppState,
    workspace_id: &str,
) -> Result<Vec<(String, String, String)>, AppError> {
    let store = state.lock_workspaces()?;
    let reg = state.lock_repo_registry()?;
    let ws = store
        .get(workspace_id)
        .ok_or_else(|| AppError::Other(format!("workspace not found: {workspace_id}")))?;
    Ok(ws.repo_ids.iter()
        .filter_map(|id| reg.get(id))
        .filter(|e| std::path::Path::new(&e.path).exists())
        .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
        .collect())
}

#[platform::handler(program = "platform")]
fn workspace_fetch_all(
    state: &AppState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = format!("Fetch workspace ({total} repos)");
    let job_cmd  = format!("workspace-fetch-all:{workspace_id}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-fetch-{jid}"))
        .spawn(move || {
            let mut ok   = 0usize;
            let mut fail = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match fetch_one(path) {
                    Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {e}"));
                        sink_emit(&sink, "arbor://workspace-fetch-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            // Notify the frontend to refresh the graph for the active tab.
            sink_emit(&sink, "arbor://workspace-fetch-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "ok": ok, "failed": fail,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn fetch thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

fn fetch_one(path: &str) -> std::result::Result<String, String> {
    let repo = git2::Repository::open(path).map_err(|e| e.to_string())?;
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    let remote_name = remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| "no remotes configured".to_string())?
        .to_string();
    let res = crate::git::remote::fetch(&repo, &remote_name).map_err(|e| e.to_string())?;
    Ok(format!("remote='{}' objects={} bytes={}", res.remote, res.received_objects, res.received_bytes))
}

#[platform::handler(program = "platform")]
fn workspace_pull_all(
    state: &AppState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = format!("Pull workspace ({total} repos)");
    let job_cmd  = format!("workspace-pull-all:{workspace_id}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-pull-{jid}"))
        .spawn(move || {
            let mut ok       = 0usize;
            let mut fail     = 0usize;
            let mut conflict = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match pull_one(path) {
                    PullOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    PullOutcome::Conflict(msg) => {
                        conflict += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  conflict — {msg}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "conflict", "error": msg,
                        }));
                    }
                    PullOutcome::Err(msg) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {msg}"));
                        sink_emit(&sink, "arbor://workspace-pull-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": msg,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {conflict} conflict, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 && conflict == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": exit_code == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink_emit(&sink, "arbor://workspace-pull-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "ok": ok, "failed": fail, "conflict": conflict,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn pull thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum PullOutcome { Ok(String), Conflict(String), Err(String) }

fn pull_one(path: &str) -> PullOutcome {
    let repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(e) => return PullOutcome::Err(e.to_string()),
    };

    // Refuse detached HEAD up front — a clear message lets the UI suggest
    // checking out a branch first.
    if let Ok(head) = repo.head() {
        if !head.is_branch() {
            return PullOutcome::Err("detached HEAD — check out a branch to pull".into());
        }
    }

    // Already mid-operation: surface as a conflict so the user knows this repo
    // needs attention before the next run.
    let gitdir = repo.path().to_path_buf();
    let has_merge = |p: &std::path::Path| p.join("MERGE_HEAD").exists()
        || p.join("REBASE_HEAD").exists()
        || p.join("CHERRY_PICK_HEAD").exists()
        || p.join("REVERT_HEAD").exists();
    if has_merge(&gitdir) {
        return PullOutcome::Conflict("repo already has an unresolved merge/rebase".into());
    }

    let remotes = match repo.remotes() {
        Ok(r) => r,
        Err(e) => return PullOutcome::Err(e.to_string()),
    };
    let remote_name = match remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
    {
        Some(n) => n.to_string(),
        None => return PullOutcome::Err("no remotes configured".into()),
    };

    match crate::git::remote::pull(&repo, &remote_name) {
        Ok(()) => PullOutcome::Ok(format!("pulled from '{remote_name}'")),
        Err(e) => {
            if has_merge(&gitdir) {
                PullOutcome::Conflict(e.to_string())
            } else {
                PullOutcome::Err(e.to_string())
            }
        }
    }
}

#[platform::handler(program = "platform")]
fn workspace_tag_all(
    state: &AppState,
    workspace_id: String,
    tag_name: String,
    message: Option<String>,
    push: bool,
) -> Result<WorkspaceFetchStartResult, AppError> {
    let trimmed = tag_name.trim().to_string();
    if trimmed.is_empty() {
        return Err(AppError::Other("tag name is required".into()));
    }

    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = if push {
        format!("Tag workspace '{trimmed}' + push ({total} repos)")
    } else {
        format!("Tag workspace '{trimmed}' ({total} repos)")
    };
    let job_cmd = format!("workspace-tag-all:{workspace_id}:{trimmed}");
    let job_id = start_workspace_job(state, &job_name, &job_cmd)?;

    let sink = state.event_sink();
    let jobs = Arc::clone(&state.jobs);
    let ws_id = workspace_id.clone();
    let jid   = job_id.clone();
    let tag   = trimmed.clone();
    let msg   = message.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-tag-{jid}"))
        .spawn(move || {
            let mut ok      = 0usize;
            let mut fail    = 0usize;
            let mut skipped = 0usize;

            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1, total = total);
                log_and_emit(&sink, &jobs, &jid, &header);
                sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                    "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));

                match tag_one(path, &tag, msg.as_deref(), push) {
                    TagOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  ok — {summary}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    TagOutcome::Skipped(reason) => {
                        skipped += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  skipped — {reason}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "skipped", "error": reason,
                        }));
                    }
                    TagOutcome::Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &jobs, &jid, &format!("  error — {e}"));
                        sink_emit(&sink, "arbor://workspace-tag-progress", serde_json::json!({
                            "job_id": &jid, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }

            let summary = format!("Done — {ok} ok, {skipped} skipped, {fail} failed, {total} total");
            log_and_emit(&sink, &jobs, &jid, &summary);

            let exit_code = if fail == 0 { 0 } else { 1 };
            if let Ok(mut j) = jobs.lock() {
                j.set_status(&jid, JobStatus::Completed { exit_code });
            }
            sink_emit(&sink, "arbor://job-done", serde_json::json!({
                "job_id": jid, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink_emit(&sink, "arbor://workspace-tag-done", serde_json::json!({
                "job_id": jid, "workspace_id": ws_id, "tag_name": tag,
                "ok": ok, "failed": fail, "skipped": skipped,
            }));
        })
        .map_err(|e| AppError::Other(format!("failed to spawn tag thread: {e}")))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum TagOutcome { Ok(String), Skipped(String), Err(String) }

fn tag_one(path: &str, tag_name: &str, message: Option<&str>, push: bool) -> TagOutcome {
    let repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    let head = match repo.head() {
        Ok(h) => h,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };
    if !head.is_branch() {
        return TagOutcome::Skipped("detached HEAD — no branch to tag".into());
    }
    let target_oid = match head.target() {
        Some(oid) => oid,
        None      => return TagOutcome::Err("HEAD has no target".into()),
    };
    let target = match repo.find_object(target_oid, Some(git2::ObjectType::Commit)) {
        Ok(o)  => o,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    let create_res = if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        match repo.signature() {
            Ok(sig) => repo.tag(tag_name, &target, &sig, msg, false).map(|_| "annotated"),
            Err(e)  => return TagOutcome::Err(e.to_string()),
        }
    } else {
        repo.tag_lightweight(tag_name, &target, false).map(|_| "lightweight")
    };
    let kind = match create_res {
        Ok(k) => k,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };

    if !push {
        return TagOutcome::Ok(format!("{kind} tag at {}", &target_oid.to_string()[..8]));
    }

    let remotes = match repo.remotes() {
        Ok(r)  => r,
        Err(e) => return TagOutcome::Err(format!("tag created locally; push skipped — {e}")),
    };
    let remote_name = match remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
    {
        Some(n) => n.to_string(),
        None    => return TagOutcome::Err("tag created locally; push skipped — no remotes configured".into()),
    };
    let refspec = format!("refs/tags/{tag_name}:refs/tags/{tag_name}");
    match crate::git::remote::push(&repo, &remote_name, &refspec, false) {
        Ok(()) => TagOutcome::Ok(format!("{kind} tag pushed to '{remote_name}'")),
        Err(e) => TagOutcome::Err(format!("tag created locally; push to '{remote_name}' failed — {e}")),
    }
}
