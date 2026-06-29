//! `mr` domain — merge/pull-request network operations, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::mr`), but the context is [`CorvusState`] and the
//! provider comes from the reverse-channel registry ([`crate::provider`])
//! instead of the shell's `GitProviderRegistry`. Each handler resolves the
//! repo's `GitProvider` ([`provider_for_tab`](crate::provider::provider_for_tab),
//! a brief sync lock returning owned `Arc`s — no guard held across the `.await`),
//! then `.await`s the provider's REST call. The trait work is the shared
//! `corvus-git-provider-{api,github,gitlab}` crates, so results and
//! `ProviderError` wire strings are identical to in-process.
//!
//! **Hooks fire here** (plugin-relocation Wave 0): `on_mr_opened` (on create) and
//! `on_mr_updated` (on close/reopen/mark-ready) go to the co-located host with a
//! byte-identical payload, fired in the same position as in-process (after the
//! provider call's result is in hand, no provider handle held across the fire).
//! Auto-merge progress notifications still ride `state.emit("plugin:notification",
//! …)` — `CorvusState::emit` forwards to the same egress.
//!
//! `merge_mr` also lives here: the provider merge is a REST call, and its GitHub
//! branch-cleanup path (`delete_remote_via_push`) opens the repo by the pushed
//! path and runs `corvus_git::branch::delete_remote_branches` with a `push`
//! closure bound to the shared `__git_credentials` resolver — the same
//! reverse-channel git auth `remote`/`notes` use. The blocking resolver round-trip
//! runs inline on the per-request worker thread (the reader thread delivers the
//! reply — no deadlock), matching the in-process flow.
//!
//! `mr_start_conflict_resolution` — the streaming-seam pilot — lives here too
//! now. It mints a job in the shell's registry over the reverse channel
//! ([`JobHandle`](crate::jobs::JobHandle)), then runs the extracted merge-prep
//! flow ([`corvus_git::merge::prepare_mr_conflict_resolution`]) on a detached
//! worker thread so the call returns the `job_id` immediately — exactly like the
//! in-process copy. The `arbor://job-*` / `arbor://mr-conflict-*` events + the
//! standardized `arbor://mr-conflict-stream` quartet are emitted through
//! `CorvusState`'s sink, byte-identical to in-process. The targeted `git fetch`
//! resolves its `-c` auth header from the reverse-channel `__git_credentials`
//! (the keyring stays shell-side), reconstructed via
//! [`corvus_git::merge::http_auth_args_for_credentials`].

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller, Stream};
use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    AutoMergeOpts, CreateMrParams, MergeOpts, MergeRequest, MergedMrHint, MrCapabilities, MrCommit,
    MrDetail, MrFeatureStatus, MrFile, MrFilter,
};
use arbor_feedback::prelude::{JobSpec, JobStatus};
use corvus_git::merge::{
    http_auth_args_for_credentials, prepare_mr_conflict_resolution, MrPrepEvent, MrPrepOutcome,
    MrPrepPhase,
};
use git2::Repository;
use serde_json::json;

use crate::jobs::JobHandle;
use crate::provider::{maybe_refresh, mr_id_from, pe, provider_for_tab};
use crate::remote::credential_resolver;
use crate::repo::open;

// ---------------------------------------------------------------------------
// List MRs / PRs
// ---------------------------------------------------------------------------

/// List pull / merge requests for the active repo.
/// `state_filter`: "open" | "closed" | "merged" | "all"
#[arbor_rpc::handler]
async fn list_mrs(
    state:        &CorvusState,
    tab_id:       String,
    state_filter: Option<String>,
) -> Result<Vec<MergeRequest>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let filter = MrFilter {
        state: Some(state_filter.unwrap_or_else(|| "open".into())),
        ..Default::default()
    };
    resolved.provider.list_mrs(&resolved.repo, filter).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Get MR detail (with comments + checks)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn get_mr_detail(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<MrDetail, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.get_mr(&id).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Create MR / PR
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn create_mr(
    state:      &CorvusState,
    tab_id:     String,
    params:     CreateMrParams,
) -> Result<MergeRequest, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);

    // Capture the flags we echo back / use for auto-merge before `params` is
    // moved into `create_mr`.
    let auto          = params.auto_merge;
    let squash        = params.squash;
    let delete_branch = params.delete_branch;

    let mut mr = resolved.provider.create_mr(&resolved.repo, params).await.map_err(pe)?;
    // The provider may not echo these creation-time preferences back on the
    // returned MR — keep them so the detail modal's merge defaults are right.
    mr.squash        = squash;
    mr.delete_branch = delete_branch;

    if auto {
        // The provider resolves any extra handle it needs internally (GitHub's
        // GraphQL node id; GitLab's wait-for-mergeable poll).
        let id = mr_id_from(&resolved, mr.number);
        match resolved.provider.enable_auto_merge(&id, AutoMergeOpts { squash, delete_branch }).await {
            Ok(())  => emit_auto_merge_ok(state, mr.number),
            Err(e)  => emit_auto_merge_err(state, mr.number, &e.to_string()),
        }
    }

    fire_mr_hook(state, "on_mr_opened", &mr);
    Ok(mr)
}

fn emit_auto_merge_ok(state: &CorvusState, number: u64) {
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge armed for #{number}"),
        "message": "Will merge automatically once required checks pass.",
        "level":   "success",
    }));
}

fn emit_auto_merge_err(state: &CorvusState, number: u64, err: &str) {
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String = if short.len() > 240 { format!("{}…", &short[..240]) } else { short.to_string() };
    state.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge could not be enabled on #{number}"),
        "message": trimmed,
        "level":   "error",
    }));
}

// ---------------------------------------------------------------------------
// Capability probe (drives "Enable auto-merge" enabled/disabled state)
// ---------------------------------------------------------------------------

/// Detect whether the active repo supports arming auto-merge / MWPS at
/// creation time.  Never fails — on any error it returns the permissive
/// default (`auto_merge_supported = true`) so the user can still try.
#[arbor_rpc::handler]
async fn get_mr_capabilities(
    state:  &CorvusState,
    tab_id: String,
) -> Result<MrCapabilities, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrCapabilities::default()),
    };
    maybe_refresh(&resolved.info.provider);

    // The provider returns the permissive default on any internal probe failure;
    // map a hard error (provider/transport) to the default too so the user can
    // still try.
    Ok(resolved.provider.auto_merge_allowed(&resolved.repo).await.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// MR/PR feature probe (drives sidebar EmptyState + Palette gating)
// ---------------------------------------------------------------------------

/// Probe whether the active repo accepts merge/pull requests at all.
/// Permissive on failure: any missing token or network error returns
/// `enabled = true` so the user can still try.  Only an explicit signal
/// from the provider (archived/disabled repo on GitHub, MR access level
/// "disabled" on GitLab) flips it to `false`.
#[arbor_rpc::handler]
async fn probe_mr_feature(
    state:  &CorvusState,
    tab_id: String,
) -> Result<MrFeatureStatus, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrFeatureStatus::default()),
    };
    maybe_refresh(&resolved.info.provider);

    // Permissive on any probe/transport failure (the failing call will surface
    // a normal error later).
    Ok(resolved.provider.mr_feature_status(&resolved.repo).await.unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Disable auto-merge
// ---------------------------------------------------------------------------

/// Cancel a previously-armed auto-merge / merge-when-pipeline-succeeds.
/// Idempotent: succeeds silently if auto-merge wasn't active.
#[arbor_rpc::handler]
async fn disable_mr_auto_merge(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.disable_auto_merge(&id).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Close / Reopen MR / PR
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn close_mr(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.close_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

#[arbor_rpc::handler]
async fn reopen_mr(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.reopen_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mark as ready for review (remove draft status)
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn mark_mr_ready(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.mark_mr_ready(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Add comment
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn add_mr_comment(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
    body:   String,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.add_mr_comment(&id, &body).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File diffs / commits
// ---------------------------------------------------------------------------

#[arbor_rpc::handler]
async fn get_mr_files(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrFile>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_files(&id).await.map_err(pe)
}

#[arbor_rpc::handler]
async fn get_mr_commits(
    state:  &CorvusState,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrCommit>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_commits(&id).await.map_err(pe)
}

#[arbor_rpc::handler]
async fn get_mr_commit_diff(
    state:  &CorvusState,
    tab_id: String,
    sha:    String,
) -> Result<Vec<MrFile>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved.provider.get_commit_diff(&resolved.repo, &sha).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Squash-merge hints for graph visualization
// ---------------------------------------------------------------------------

/// Returns a list of `{ sourceBranch, mergeCommitSha }` for all merged
/// PRs/MRs.  Used by the graph to draw accurate ghost edges for squash merges.
/// Returns an empty Vec (never errors) when the provider is not configured or
/// the token is missing — the graph simply shows no ghost edges in that case.
#[arbor_rpc::handler]
async fn get_merged_mr_hints(
    state:  &CorvusState,
    tab_id: String,
) -> Result<Vec<MergedMrHint>, String> {
    let resolved = match provider_for_tab(state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(vec![]),
    };
    maybe_refresh(&resolved.info.provider);

    if !resolved.provider.has_token() {
        return Ok(vec![]);
    }

    // GitHub: ask for "closed" (merged is a subset); GitLab: ask for "merged" directly.
    let state_filter = match resolved.info.provider.as_str() {
        "github" => "closed",
        _        => "merged",
    };
    let filter = MrFilter { state: Some(state_filter.into()), ..Default::default() };
    let mrs = match resolved.provider.list_mrs(&resolved.repo, filter).await {
        Ok(v)  => v,
        Err(_) => return Ok(vec![]),
    };

    let hints = mrs
        .into_iter()
        .filter_map(|mr| {
            let merge_sha = mr.merge_commit_sha?;
            Some(MergedMrHint {
                source_branch:    mr.source_branch,
                merge_commit_sha: merge_sha,
                head_sha:         mr.head_sha,
                base_sha:         mr.base_sha,
            })
        })
        .collect();

    Ok(hints)
}

// ---------------------------------------------------------------------------
// Merge MR / PR
// ---------------------------------------------------------------------------

/// Merge a pull/merge request, then (GitHub + delete requested) delete the
/// source branch on the remote via `git push :refs/heads/<branch>`, and fire
/// `on_mr_merged`. Byte-identical to the in-process copy.
#[arbor_rpc::handler]
async fn merge_mr(
    state:         &CorvusState,
    tab_id:        String,
    number:        u64,
    merge_method:  Option<String>,
    squash:        Option<bool>,
    delete_branch: Option<bool>,
    source_branch: Option<String>,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    let do_squash = squash.unwrap_or(false);
    let do_delete = delete_branch.unwrap_or(false);
    let strategy = if do_squash { Some("squash".into()) } else { merge_method };

    let id = mr_id_from(&resolved, number);
    let opts = MergeOpts {
        squash:         do_squash,
        delete_branch:  do_delete,
        commit_title:   None,
        commit_message: None,
        strategy,
    };
    resolved.provider.merge_mr(&id, opts).await.map_err(pe)?;

    // GitHub: delete the remote branch via `git push :refs/heads/<branch>` because
    // the REST DELETE endpoint has been observed to silently no-op under some
    // OAuth/App configurations. GitLab handles delete server-side via `do_delete`.
    if resolved.info.provider == "github" && do_delete {
        match source_branch.as_deref() {
            Some(branch) => {
                delete_remote_via_push(state, &tab_id, &resolved.info.remote_url, number, branch)
            }
            None => emit_remote_delete_warning(
                state,
                number,
                "?",
                "no source branch was provided — cannot delete remote branch.",
            ),
        }
    }

    fire_mr_hook_by_number(state, "on_mr_merged", number, &resolved.info.provider);
    Ok(())
}

/// Delete `<branch>` on the remote whose URL is `remote_url` using
/// `git push :refs/heads/<branch>`, authenticated by the shared
/// `__git_credentials` resolver. Failures surface as a non-fatal warning toast,
/// exactly as in-process.
fn delete_remote_via_push(
    state:      &CorvusState,
    tab_id:     &str,
    remote_url: &str,
    number:     u64,
    branch:     &str,
) {
    let outcome: Result<Vec<String>, String> = (|| {
        let repo = open(state, tab_id)?;
        let remote_name = corvus_git::remote::list_remotes(&repo)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|r| r.url == remote_url)
            .map(|r| r.name)
            .unwrap_or_else(|| "origin".into());
        let qualified = format!("{remote_name}/{branch}");

        let host = state
            .host_caller()
            .ok_or_else(|| "merge_mr: no reverse channel for credentials".to_string())?;
        let resolver = credential_resolver(host);
        let push = move |repo: &Repository, remote: &str, refspec: &str, force: bool| {
            corvus_git::remote::push(repo, remote, refspec, force, &resolver).map_err(|e| e.to_string())
        };
        Ok(corvus_git::branch::delete_remote_branches(&repo, &[qualified], &push))
    })();
    match outcome {
        Ok(failed) if failed.is_empty() => {} // success → silent; sidebar refresh shows it gone
        Ok(_) => emit_remote_delete_warning(
            state,
            number,
            branch,
            "git push --delete failed (check the remote and your credentials)",
        ),
        Err(e) => emit_remote_delete_warning(state, number, branch, &e),
    }
}

fn emit_remote_delete_warning(state: &CorvusState, number: u64, branch: &str, err: &str) {
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String =
        if short.len() > 400 { format!("{}…", &short[..400]) } else { short.to_string() };
    state.emit(
        "plugin:notification",
        serde_json::json!({
            "plugin":  "arbor",
            "title":   format!("Remote branch '{branch}' not deleted after merging #{number}"),
            "message": trimmed,
            "level":   "warning",
        }),
    );
}

// ---------------------------------------------------------------------------
// Hook firing helpers
// ---------------------------------------------------------------------------

fn fire_mr_hook(
    state: &CorvusState,
    hook:  &str,
    mr:    &MergeRequest,
) {
    let ctx = serde_json::json!({
        "number":        mr.number,
        "title":         mr.title,
        "source_branch": mr.source_branch,
        "target_branch": mr.target_branch,
        "provider":      mr.provider,
        "author":        mr.author.login,
        "web_url":       mr.web_url,
    });
    state.fire_hook(hook, ctx);
}

fn fire_mr_hook_by_number(
    state:    &CorvusState,
    hook:     &str,
    number:   u64,
    provider: &str,
) {
    let ctx = serde_json::json!({ "number": number, "provider": provider });
    state.fire_hook(hook, ctx);
}

// ---------------------------------------------------------------------------
// Start MR conflict resolution — streaming-seam pilot
// ---------------------------------------------------------------------------

/// Prepare the local workspace to resolve a pull/merge-request conflict.
///
/// Mints a job in the shell registry (over the reverse channel) and runs the
/// multi-step prep flow on a detached worker thread, returning the `job_id`
/// immediately. Progress is reported byte-identically to the in-process copy:
///
/// - `arbor://job-started` / `arbor://job-output` / `arbor://job-done` — the
///   standard job lifecycle (per-phase headers + per-line git output appear in
///   the Job Output panel).
/// - `arbor://mr-conflict-progress` / `arbor://mr-conflict-done` — the typed
///   ProgressStepper feed + terminal `{ status: "clean" | "conflicts" | "error" }`.
/// - the additive standardized `arbor://mr-conflict-stream-*` quartet
///   (`stream_id == job_id`).
#[arbor_rpc::handler]
fn mr_start_conflict_resolution(
    state:         &CorvusState,
    tab_id:        String,
    source_branch: String,
    target_branch: String,
) -> Result<String, String> {
    // Egress + reverse channel captured once — the worker holds no `&CorvusState`.
    let sink: Arc<dyn EventSink> = state.event_sink();
    let host: Arc<dyn HostCaller> = state
        .host_caller()
        .ok_or_else(|| "mr_start_conflict_resolution: no reverse channel".to_string())?;

    let workdir = {
        let repo = open(state, &tab_id)?;
        repo.workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf()
    };

    let name    = format!("Resolve conflicts: {source_branch} ← {target_branch}");
    let command = format!("git fetch + checkout {source_branch} + merge origin/{target_branch}");

    let job = JobHandle::register(Arc::clone(&host), JobSpec {
        name:            name.clone(),
        plugin_name:     "arbor".to_string(),
        command:         command.clone(),
        category:        Some("Merge".to_string()),
        non_cancellable: true,
        hidden:          false,
        is_system:       false,
        target:          None,
    })?;
    let job_id = job.id.clone();

    // `stream_id == job_id`: one identity addresses the Jobs entry, the stream
    // quartet, and (where applicable) cancellation.
    let stream = Stream::new(Arc::clone(&sink), "arbor://mr-conflict-stream", job_id.clone());

    // `job-started` — byte-identical to the in-process emit.
    sink.emit("arbor://job-started", json!({
        "job_id":      &job_id,
        "name":        &name,
        "plugin_name": "arbor",
        "command":     &command,
        "category":    "Merge",
    }));
    // Additive standardized lifecycle.
    stream.started(json!({ "phase_total": MrPrepPhase::TOTAL }));

    // The git invoker uses the shell-pushed program (same as every other OOP
    // git op in this process).
    let git = crate::repo::git(state);

    let jid       = job_id.clone();
    let sink_bg   = Arc::clone(&sink);
    let stream_bg = stream.clone();
    let host_bg   = Arc::clone(&host);

    let spawn_result = std::thread::Builder::new()
        .name(format!("corvus-mr-conflict-{}", jid))
        .spawn(move || {
            // The targeted `git fetch`'s `-c` auth header is reconstructed from
            // the reverse-channel `__git_credentials` pair (keyring shell-side).
            // Runs inline on this worker — the reader thread delivers the reply,
            // so the blocking host round-trip cannot deadlock.
            let resolve_auth = |url: &str| -> Vec<String> {
                let creds: Option<(String, String)> = host_bg
                    .call("__git_credentials", json!(url))
                    .ok()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .flatten();
                match creds {
                    Some((user, secret)) => http_auth_args_for_credentials(url, &user, &secret),
                    None => Vec::new(),
                }
            };

            let result = prepare_mr_conflict_resolution(
                &git,
                &workdir,
                &source_branch,
                &target_branch,
                &resolve_auth,
                |evt| match evt {
                    MrPrepEvent::PhaseStart { phase, detail } => {
                        sink_bg.emit("arbor://mr-conflict-progress", json!({
                            "job_id":      &jid,
                            "phase":       phase.key(),
                            "phase_index": phase.index(),
                            "phase_total": MrPrepPhase::TOTAL,
                            "label":       phase.label(),
                            "detail":      detail,
                        }));
                        let header = match &detail {
                            Some(d) => format!("── {} ({})", phase.label(), d),
                            None    => format!("── {}", phase.label()),
                        };
                        job.append(&header);
                        sink_bg.emit("arbor://job-output", json!({
                            "job_id": &jid, "text": header,
                        }));
                        // Additive standardized per-phase chunk.
                        stream_bg.chunk(json!({
                            "phase":       phase.key(),
                            "phase_index": phase.index(),
                            "phase_total": MrPrepPhase::TOTAL,
                            "label":       phase.label(),
                            "detail":      detail,
                        }));
                    }
                    MrPrepEvent::Output { phase: _, line } => {
                        job.append(line);
                        sink_bg.emit("arbor://job-output", json!({
                            "job_id": &jid, "text": line,
                        }));
                    }
                },
            );

            let (status_payload, outcome_label, error_msg) = match &result {
                Ok(MrPrepOutcome::Clean)     => (Ok(0i32), "clean",     None),
                Ok(MrPrepOutcome::Conflicts) => (Ok(0i32), "conflicts", None),
                Err(e)                       => (Err(()), "error",      Some(e.to_string())),
            };

            let status = match status_payload {
                Ok(c)  => JobStatus::Completed { exit_code: c },
                Err(_) => JobStatus::Failed { error: error_msg.clone().unwrap_or_default() },
            };
            job.set_status(status);

            sink_bg.emit("arbor://job-done", json!({
                "job_id":    &jid,
                "success":   status_payload.is_ok(),
                "exit_code": status_payload.unwrap_or(-1),
            }));

            sink_bg.emit("arbor://mr-conflict-done", json!({
                "job_id": &jid,
                "status": outcome_label,
                "error":  error_msg,
            }));

            // Additive standardized terminal event.
            match status_payload {
                Ok(_)  => stream_bg.done(json!({ "status": outcome_label })),
                Err(_) => stream_bg.error(error_msg.as_deref().unwrap_or("error")),
            }
        });

    if let Err(e) = spawn_result {
        let err = format!("failed to spawn mr-conflict thread: {e}");
        // The original `JobHandle` moved into the (never-run) closure, so flip
        // the registry entry to Failed by the same id directly over the reverse
        // channel — `__job_set_status` is exactly what `JobHandle::set_status`
        // calls. Then emit the same terminal trio the in-process spawn-failure
        // path emits.
        let _ = host.call("__job_set_status", json!({
            "job_id": &job_id,
            "status": JobStatus::Failed { error: err.clone() },
        }));
        sink.emit("arbor://job-done", json!({
            "job_id":    &job_id,
            "success":   false,
            "exit_code": -1,
        }));
        sink.emit("arbor://mr-conflict-done", json!({
            "job_id": &job_id,
            "status": "error",
            "error":  err.clone(),
        }));
        stream.error(&err);
        return Err(err);
    }

    Ok(job_id)
}
