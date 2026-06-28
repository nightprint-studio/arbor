//! `workspace` background runners — fetch-all / pull-all / tag-all, served
//! **out-of-process** by corvus-be.
//!
//! Ported from the shell's `crate::ipc::platform::workspace_runs`. Each handler
//! registers a system job in the shell's registry (over the reverse channel via
//! [`JobHandle`](crate::jobs::JobHandle) — ADR-3), spawns one background thread,
//! and streams `arbor://job-*` / `arbor://workspace-*` progress through the
//! backend [`EventSink`]. The per-repo git work is the shared [`corvus_git`]
//! crate; git smart-HTTP credentials resolve over the reverse channel
//! ([`crate::remote::credential_resolver`]), the same pair the single-tab
//! fetch/push handlers use. Errors never abort the run — collected + reported.

use std::sync::Arc;

use arbor_feedback::prelude::{JobSpec, JobStatus};
use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use corvus_git::prelude::GitCli;
use serde_json::json;

use crate::jobs::JobHandle;
use crate::remote::credential_resolver;
use crate::repo::git;
use crate::workspace::{registry, store, WorkspaceFetchStartResult};

/// Append a line to the job buffer (registry) and mirror it to the Jobs overlay.
fn log_and_emit(sink: &Arc<dyn EventSink>, job: &JobHandle, line: &str) {
    job.append(line);
    sink.emit("arbor://job-output", json!({ "job_id": job.id, "text": line }));
}

/// Register a system job for a workspace-wide run + emit `arbor://job-started`.
fn start_workspace_job(
    state: &CorvusState,
    job_name: &str,
    job_cmd: &str,
) -> Result<JobHandle, String> {
    let host = state
        .host_caller()
        .ok_or_else(|| "workspace run: no reverse channel".to_string())?;
    let job = JobHandle::register(host, JobSpec {
        name:            job_name.to_string(),
        plugin_name:     "arbor".into(),
        command:         job_cmd.to_string(),
        category:        Some("System".into()),
        non_cancellable: false,
        hidden:          false,
        is_system:       true,
        target:          None,
    })?;
    state.emit("arbor://job-started", json!({
        "job_id":      &job.id,
        "name":        job_name,
        "plugin_name": "arbor",
        "command":     job_cmd,
        "category":    "System",
    }));
    Ok(job)
}

/// Freeze the (repo_id, path, display_name) targets of a workspace's existing
/// on-disk repos. Reads the reload-on-access store + registry.
fn workspace_targets(
    state: &CorvusState,
    workspace_id: &str,
) -> Result<Vec<(String, String, String)>, String> {
    let store = store::store(state);
    let reg = registry::registry(state);
    let ws = store
        .get(workspace_id)
        .ok_or_else(|| format!("workspace not found: {workspace_id}"))?;
    Ok(ws.repo_ids.iter()
        .filter_map(|id| reg.get(id))
        .filter(|e| std::path::Path::new(&e.path).exists())
        .map(|e| (e.id.clone(), e.path.clone(), e.display_name.clone()))
        .collect())
}

#[arbor_rpc::handler]
fn workspace_fetch_all(
    state: &CorvusState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, String> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job = start_workspace_job(
        state,
        &format!("Fetch workspace ({total} repos)"),
        &format!("workspace-fetch-all:{workspace_id}"),
    )?;
    let job_id = job.id.clone();

    let sink = state.event_sink();
    let host = state.host_caller().ok_or_else(|| "no reverse channel".to_string())?;
    let ws_id = workspace_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-fetch-{job_id}"))
        .spawn(move || {
            let resolver = credential_resolver(host);
            let mut ok = 0usize;
            let mut fail = 0usize;
            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1);
                log_and_emit(&sink, &job, &header);
                sink.emit("arbor://workspace-fetch-progress", json!({
                    "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));
                match fetch_one(path, &resolver) {
                    Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &job, &format!("  ok — {summary}"));
                        sink.emit("arbor://workspace-fetch-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &job, &format!("  error — {e}"));
                        sink.emit("arbor://workspace-fetch-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }
            let summary = format!("Done — {ok} ok, {fail} failed, {total} total");
            log_and_emit(&sink, &job, &summary);
            let exit_code = if fail == 0 { 0 } else { 1 };
            job.set_status(JobStatus::Completed { exit_code });
            sink.emit("arbor://job-done", json!({
                "job_id": job.id, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink.emit("arbor://workspace-fetch-done", json!({
                "job_id": job.id, "workspace_id": ws_id, "ok": ok, "failed": fail,
            }));
        })
        .map_err(|e| format!("failed to spawn fetch thread: {e}"))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

fn fetch_one<R>(path: &str, resolver: &R) -> Result<String, String>
where
    R: Fn(&str) -> Result<Option<(String, String)>, String> + Send + Sync,
{
    let repo = git2::Repository::open(path).map_err(|e| e.to_string())?;
    let remotes = repo.remotes().map_err(|e| e.to_string())?;
    let remote_name = remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| "no remotes configured".to_string())?
        .to_string();
    let res = corvus_git::remote::fetch(&repo, &remote_name, resolver).map_err(|e| e.to_string())?;
    Ok(format!("remote='{}' objects={} bytes={}", res.remote, res.received_objects, res.received_bytes))
}

#[arbor_rpc::handler]
fn workspace_pull_all(
    state: &CorvusState,
    workspace_id: String,
) -> Result<WorkspaceFetchStartResult, String> {
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job = start_workspace_job(
        state,
        &format!("Pull workspace ({total} repos)"),
        &format!("workspace-pull-all:{workspace_id}"),
    )?;
    let job_id = job.id.clone();

    let sink = state.event_sink();
    let host = state.host_caller().ok_or_else(|| "no reverse channel".to_string())?;
    let invoker = git(state);
    let ws_id = workspace_id.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-pull-{job_id}"))
        .spawn(move || {
            let resolver = credential_resolver(host);
            let mut ok = 0usize;
            let mut fail = 0usize;
            let mut conflict = 0usize;
            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1);
                log_and_emit(&sink, &job, &header);
                sink.emit("arbor://workspace-pull-progress", json!({
                    "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));
                match pull_one(&invoker, path, &resolver) {
                    PullOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &job, &format!("  ok — {summary}"));
                        sink.emit("arbor://workspace-pull-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    PullOutcome::Conflict(msg) => {
                        conflict += 1;
                        log_and_emit(&sink, &job, &format!("  conflict — {msg}"));
                        sink.emit("arbor://workspace-pull-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "conflict", "error": msg,
                        }));
                    }
                    PullOutcome::Err(msg) => {
                        fail += 1;
                        log_and_emit(&sink, &job, &format!("  error — {msg}"));
                        sink.emit("arbor://workspace-pull-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": msg,
                        }));
                    }
                }
            }
            let summary = format!("Done — {ok} ok, {conflict} conflict, {fail} failed, {total} total");
            log_and_emit(&sink, &job, &summary);
            let exit_code = if fail == 0 && conflict == 0 { 0 } else { 1 };
            job.set_status(JobStatus::Completed { exit_code });
            sink.emit("arbor://job-done", json!({
                "job_id": job.id, "success": exit_code == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink.emit("arbor://workspace-pull-done", json!({
                "job_id": job.id, "workspace_id": ws_id, "ok": ok, "failed": fail, "conflict": conflict,
            }));
        })
        .map_err(|e| format!("failed to spawn pull thread: {e}"))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum PullOutcome { Ok(String), Conflict(String), Err(String) }

fn pull_one<R>(invoker: &GitCli, path: &str, resolver: &R) -> PullOutcome
where
    R: Fn(&str) -> Result<Option<(String, String)>, String> + Send + Sync,
{
    let mut repo = match git2::Repository::open(path) {
        Ok(r) => r,
        Err(e) => return PullOutcome::Err(e.to_string()),
    };
    // Refuse detached HEAD up front.
    if let Ok(head) = repo.head() {
        if !head.is_branch() {
            return PullOutcome::Err("detached HEAD — check out a branch to pull".into());
        }
    }
    // Already mid-operation → surface as a conflict.
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
    match corvus_git::remote::pull(invoker, &mut repo, &remote_name, resolver) {
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

#[arbor_rpc::handler]
fn workspace_tag_all(
    state: &CorvusState,
    workspace_id: String,
    tag_name: String,
    message: Option<String>,
    push: bool,
) -> Result<WorkspaceFetchStartResult, String> {
    let trimmed = tag_name.trim().to_string();
    if trimmed.is_empty() {
        return Err("tag name is required".into());
    }
    let targets = workspace_targets(state, &workspace_id)?;
    let total = targets.len();
    let job_name = if push {
        format!("Tag workspace '{trimmed}' + push ({total} repos)")
    } else {
        format!("Tag workspace '{trimmed}' ({total} repos)")
    };
    let job = start_workspace_job(state, &job_name, &format!("workspace-tag-all:{workspace_id}:{trimmed}"))?;
    let job_id = job.id.clone();

    let sink = state.event_sink();
    let host = state.host_caller().ok_or_else(|| "no reverse channel".to_string())?;
    let ws_id = workspace_id.clone();
    let tag = trimmed.clone();
    let msg = message.clone();
    std::thread::Builder::new()
        .name(format!("arbor-workspace-tag-{job_id}"))
        .spawn(move || {
            let resolver = credential_resolver(host);
            let mut ok = 0usize;
            let mut fail = 0usize;
            let mut skipped = 0usize;
            for (idx, (repo_id, path, display_name)) in targets.iter().enumerate() {
                let header = format!("[{n}/{total}] {display_name} — {path}", n = idx + 1);
                log_and_emit(&sink, &job, &header);
                sink.emit("arbor://workspace-tag-progress", json!({
                    "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                    "index": idx, "total": total, "phase": "start",
                }));
                match tag_one(path, &tag, msg.as_deref(), push, &resolver) {
                    TagOutcome::Ok(summary) => {
                        ok += 1;
                        log_and_emit(&sink, &job, &format!("  ok — {summary}"));
                        sink.emit("arbor://workspace-tag-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "ok",
                        }));
                    }
                    TagOutcome::Skipped(reason) => {
                        skipped += 1;
                        log_and_emit(&sink, &job, &format!("  skipped — {reason}"));
                        sink.emit("arbor://workspace-tag-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "skipped", "error": reason,
                        }));
                    }
                    TagOutcome::Err(e) => {
                        fail += 1;
                        log_and_emit(&sink, &job, &format!("  error — {e}"));
                        sink.emit("arbor://workspace-tag-progress", json!({
                            "job_id": &job.id, "workspace_id": &ws_id, "repo_id": repo_id,
                            "index": idx, "total": total, "phase": "error", "error": e,
                        }));
                    }
                }
            }
            let summary = format!("Done — {ok} ok, {skipped} skipped, {fail} failed, {total} total");
            log_and_emit(&sink, &job, &summary);
            let exit_code = if fail == 0 { 0 } else { 1 };
            job.set_status(JobStatus::Completed { exit_code });
            sink.emit("arbor://job-done", json!({
                "job_id": job.id, "success": fail == 0, "exit_code": exit_code, "summary": summary,
            }));
            sink.emit("arbor://workspace-tag-done", json!({
                "job_id": job.id, "workspace_id": ws_id, "tag_name": tag,
                "ok": ok, "failed": fail, "skipped": skipped,
            }));
        })
        .map_err(|e| format!("failed to spawn tag thread: {e}"))?;

    Ok(WorkspaceFetchStartResult { job_id, total })
}

enum TagOutcome { Ok(String), Skipped(String), Err(String) }

fn tag_one<R>(
    path: &str,
    tag_name: &str,
    message: Option<&str>,
    push: bool,
    resolver: &R,
) -> TagOutcome
where
    R: Fn(&str) -> Result<Option<(String, String)>, String> + Send + Sync,
{
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
        None => return TagOutcome::Err("HEAD has no target".into()),
    };
    let target = match repo.find_object(target_oid, Some(git2::ObjectType::Commit)) {
        Ok(o) => o,
        Err(e) => return TagOutcome::Err(e.to_string()),
    };
    let create_res = if let Some(msg) = message.filter(|m| !m.trim().is_empty()) {
        match repo.signature() {
            Ok(sig) => repo.tag(tag_name, &target, &sig, msg, false).map(|_| "annotated"),
            Err(e) => return TagOutcome::Err(e.to_string()),
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
        Ok(r) => r,
        Err(e) => return TagOutcome::Err(format!("tag created locally; push skipped — {e}")),
    };
    let remote_name = match remotes.iter().flatten().find(|n| *n == "origin")
        .or_else(|| remotes.iter().flatten().next())
    {
        Some(n) => n.to_string(),
        None => return TagOutcome::Err("tag created locally; push skipped — no remotes configured".into()),
    };
    let refspec = format!("refs/tags/{tag_name}:refs/tags/{tag_name}");
    match corvus_git::remote::push(&repo, &remote_name, &refspec, false, resolver) {
        Ok(()) => TagOutcome::Ok(format!("{kind} tag pushed to '{remote_name}'")),
        Err(e) => TagOutcome::Err(format!("tag created locally; push to '{remote_name}' failed — {e}")),
    }
}
