//! `remote` domain — network remote operations, served **out-of-process** by
//! corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::remote`), but the context is [`CorvusState`]: the
//! repo is opened by the shell-pushed path and the git work is the shared
//! [`corvus_git`] crate, so behaviour + error strings are identical (the crate's
//! `GitError` `Display` mirrors `AppError`'s variant-for-variant, so
//! `e.to_string()` is the same wire string the shell produces).
//!
//! **Git smart-HTTP credentials cross the reverse channel.** The keyring is
//! shell-side, so the [`credential_resolver`] marshals `(url) -> (user, pass)`
//! to the shell's `__git_credentials` host method (HTTP-Basic, distinct from the
//! REST `AuthSession` of `__session`). The proactive `maybe_refresh_for_url`
//! pre-call is preserved as the `__maybe_refresh_url` host call. The blocking
//! fetch/push/pull runs on a `spawn_blocking` worker; libgit2 invokes the
//! resolver from there, which blocks on the reverse-channel reply — the
//! reentrancy the channel is built for.
//!
//! **Hooks fire here** (plugin-relocation Wave 0): `corvus:fetch`, `corvus:push`, and —
//! only on a clean pull — `corvus:pull`, with payloads identical to in-process.
//!
//! `pull_branch` carries the same safe-pull orchestration as in-process
//! (recovery snapshot → pre-pull stash → fetch/merge → re-apply stash) and
//! streams `arbor://pull-progress` / `arbor://pull-done` through the backend
//! [`EventSink`] (which `CorvusState` owns). The config-dependent recovery policy
//! is the shell-pushed one ([`crate::repo::snapshot_policy`]).

use std::path::Path;
use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller};
use corvus_core::prelude::{hooks, CorvusState};
use corvus_git::prelude::{
    CredentialResolver, FetchResult, GitCli, RecoveryKind, RemoteInfo, SnapshotPolicy, StashEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::repo::{git, open, repo_path, snapshot_policy};

/// Build the git smart-HTTP credential resolver bound to the reverse channel:
/// `(url) -> Option<(user, pass)>` by calling the shell's `__git_credentials`.
/// On a transport error it returns `Err` (which `corvus_git::remote` logs and
/// treats as "no stored credentials", falling through to git's own helper) —
/// the same shape the shell's `resolve_credentials` binding has. Shared with the
/// `notes` domain's namespace push.
pub(crate) fn credential_resolver(
    host: Arc<dyn HostCaller>,
) -> impl Fn(&str) -> Result<Option<(String, String)>, String> + Send + Sync {
    move |url: &str| {
        let value = host.call("__git_credentials", json!(url))?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

/// The remote's URL (for the proactive refresh), or empty if unresolved — same
/// best-effort lookup as the in-process handler (never an error).
fn remote_url(repo_path: &str, remote: &str) -> String {
    git2::Repository::open(repo_path)
        .ok()
        .and_then(|r| r.find_remote(remote).ok().and_then(|rm| rm.url().map(|s| s.to_string())))
        .unwrap_or_default()
}

#[arbor_rpc::handler]
fn list_remotes(state: &CorvusState, tab_id: String) -> Result<Vec<RemoteInfo>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::remote::list_remotes(&repo).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
async fn fetch_remote(
    state: &CorvusState,
    tab_id: String,
    remote: String,
) -> Result<FetchResult, String> {
    let path = repo_path(state, &tab_id)?;
    let url = remote_url(&path, &remote);

    // Proactive token refresh over the reverse channel (best-effort), then run
    // the blocking fetch on a worker so this process's serve loop stays
    // responsive — the credential callback calls back to the shell from there.
    let _ = state.host_call("__maybe_refresh_url", json!(url));
    let host = state
        .host_caller()
        .ok_or_else(|| "fetch_remote: no reverse channel".to_string())?;

    let remote_for_task = remote.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<FetchResult, String> {
        let repo = git2::Repository::open(&path).map_err(|e| format!("Git error: {e}"))?;
        corvus_git::remote::fetch(&repo, &remote_for_task, &credential_resolver(host))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("fetch task panicked: {e}"))??;

    state.fire_hook(hooks::FETCH, json!({ "tab_id": &tab_id, "remote": &remote }));
    Ok(result)
}

#[arbor_rpc::handler]
async fn push_branch(
    state: &CorvusState,
    tab_id: String,
    remote: String,
    refspec: String,
    force: bool,
) -> Result<(), String> {
    let path = repo_path(state, &tab_id)?;
    let url = remote_url(&path, &remote);

    let _ = state.host_call("__maybe_refresh_url", json!(url));
    let host = state
        .host_caller()
        .ok_or_else(|| "push_branch: no reverse channel".to_string())?;

    let remote_task = remote.clone();
    let refspec_task = refspec.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let repo = git2::Repository::open(&path).map_err(|e| format!("Git error: {e}"))?;
        corvus_git::remote::push(&repo, &remote_task, &refspec_task, force, &credential_resolver(host))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("push task panicked: {e}"))??;

    state.fire_hook(
        hooks::PUSH,
        json!({ "tab_id": &tab_id, "remote": &remote, "refspec": &refspec, "force": force }),
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Pull (safe flow: recovery snapshot → pre-pull stash → fetch/merge → re-apply)
// ---------------------------------------------------------------------------

/// Returned by `pull_branch` so the frontend knows whether a pre-pull stash
/// needed to be re-applied and whether that re-apply had conflicts. Serde shape
/// is byte-identical to the shell's in-process `PullResult`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    /// Relative paths of files that conflicted when re-applying the stash.
    pub stash_conflicts: Vec<String>,
    /// The stash entry created before pulling, if any (index 0 when present).
    pub pre_pull_stash: Option<StashEntry>,
    /// Non-None when the stash re-apply failed for a non-conflict reason.
    pub stash_apply_error: Option<String>,
    /// Non-None when the pull fetch/merge itself failed.
    pub pull_error: Option<String>,
}

/// `op_id`: optional opaque id correlated by the frontend OperationsOverlay.
/// When `Some`, the pull emits `arbor://pull-progress` events keyed by that id
/// plus a final `arbor://pull-done`.
#[arbor_rpc::handler]
async fn pull_branch(
    state: &CorvusState,
    tab_id: String,
    remote: String,
    op_id: Option<String>,
) -> Result<PullResult, String> {
    // Extract repo path, workdir and remote URL up front (the blocking task
    // re-opens the repo on its own, just like in-process).
    let path = repo_path(state, &tab_id)?;
    let (workdir, url) = {
        let repo = open(state, &tab_id)?;
        let wd = repo
            .workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf();
        let url = repo
            .find_remote(&remote)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()))
            .unwrap_or_default();
        (wd, url)
    };

    let _ = state.host_call("__maybe_refresh_url", json!(url));
    let host = state
        .host_caller()
        .ok_or_else(|| "pull_branch: no reverse channel".to_string())?;
    let invoker = git(state);
    let policy = snapshot_policy(state);

    // The progress side-table carries the backend event sink (Send + 'static)
    // into the blocking task. `None` when no op_id was given.
    let progress: Option<(Arc<dyn EventSink>, String)> =
        op_id.clone().map(|oid| (state.event_sink(), oid));

    let remote_task = remote.clone();
    let result: Result<PullResult, String> = tokio::task::spawn_blocking(move || {
        let mut r = git2::Repository::open(&path).map_err(|e| format!("Git error: {e}"))?;
        let resolver = credential_resolver(host);
        pull_branch_inner(&invoker, &mut r, &workdir, &remote_task, &resolver, &policy, progress.as_ref())
    })
    .await
    .map_err(|e| format!("pull task panicked: {e}"))?;

    // Always emit pull-done so the OperationsOverlay finalises the card.
    if let Some(ref oid) = op_id {
        match &result {
            Ok(pr) => {
                let (status, error) = if let Some(err) = &pr.pull_error {
                    ("error", Some(err.clone()))
                } else if !pr.stash_conflicts.is_empty() {
                    ("conflict", Some(format!(
                        "Stash apply produced conflicts on {} file(s)",
                        pr.stash_conflicts.len(),
                    )))
                } else if let Some(err) = &pr.stash_apply_error {
                    ("conflict", Some(err.clone()))
                } else {
                    ("ok", None)
                };
                state.emit("arbor://pull-done", json!({
                    "op_id":   oid,
                    "status":  status,
                    "error":   error,
                    "summary": serde_json::Value::Null,
                }));
            }
            Err(e) => {
                state.emit("arbor://pull-done", json!({
                    "op_id":   oid,
                    "status":  "error",
                    "error":   e,
                    "summary": serde_json::Value::Null,
                }));
            }
        }
    }

    let pr = result?;

    // Fire corvus:pull only on clean success (no stash apply error, no pull error,
    // no conflicts).
    if pr.pull_error.is_none() && pr.stash_apply_error.is_none() && pr.stash_conflicts.is_empty() {
        state.fire_hook(hooks::PULL, json!({ "tab_id": &tab_id, "remote": &remote }));
    }

    // If the pull failed AND there's no stash context to communicate, surface it
    // as an Err so the frontend toasts a simple error. Otherwise the PullResult
    // carries everything the frontend needs to drive recovery UI.
    if let Some(err) = pr.pull_error.clone() {
        if pr.pre_pull_stash.is_none() && pr.stash_conflicts.is_empty() {
            return Err(err);
        }
    }
    Ok(pr)
}

// Pull phases — emitted as `arbor://pull-progress` when the caller passes an
// op_id. Drives the OperationsOverlay ProgressStepper.
#[derive(Clone, Copy)]
enum PullPhase { Status, Stash, Fetch, Merge, Unstash }

impl PullPhase {
    fn key(self) -> &'static str {
        match self {
            Self::Status  => "status",
            Self::Stash   => "stash",
            Self::Fetch   => "fetch",
            Self::Merge   => "merge",
            Self::Unstash => "unstash",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Status  => "Checking workdir",
            Self::Stash   => "Stashing local changes",
            Self::Fetch   => "Fetching from origin",
            Self::Merge   => "Merging / fast-forward",
            Self::Unstash => "Restoring stash",
        }
    }
}

/// Side-table that lets `pull_branch_inner` emit progress without juggling
/// closures. When `None`, all `emit_*` calls are silent.
type PullProgress<'a> = Option<&'a (Arc<dyn EventSink>, String)>;

fn emit_phase(progress: PullProgress<'_>, phase: PullPhase, detail: Option<&str>) {
    let Some((sink, oid)) = progress else { return };
    sink.emit("arbor://pull-progress", json!({
        "op_id":   oid,
        "phase":   phase.key(),
        "label":   phase.label(),
        "detail":  detail,
        "skipped": false,
    }));
}

fn emit_phase_skipped(progress: PullProgress<'_>, phase: PullPhase, detail: Option<&str>) {
    let Some((sink, oid)) = progress else { return };
    sink.emit("arbor://pull-progress", json!({
        "op_id":   oid,
        "phase":   phase.key(),
        "label":   phase.label(),
        "detail":  detail,
        "skipped": true,
    }));
}

/// Synchronous safe-pull flow, run on the blocking pool. Mirrors the shell's
/// in-process `pull_branch_inner`: the injected pieces (the git invoker, the
/// recovery policy, the credential resolver) are passed explicitly rather than
/// read from shell globals.
#[allow(clippy::too_many_arguments)]
fn pull_branch_inner(
    invoker: &GitCli,
    r: &mut git2::Repository,
    workdir: &Path,
    remote: &str,
    resolver: CredentialResolver<'_>,
    policy: &SnapshotPolicy,
    progress: PullProgress<'_>,
) -> Result<PullResult, String> {
    // Dirtiness is decided via `git status --porcelain` (CLI), NOT libgit2's
    // `statuses()`: this handler opens a FRESH `Repository` on a cold stat cache,
    // where libgit2 can return a false-clean reading on Windows. The CLI is the
    // canonical answer and matches what the user sees from their shell.
    emit_phase(progress, PullPhase::Status, None);
    let is_dirty = {
        let out = invoker
            .command()
            .args(["status", "--porcelain"])
            .current_dir(workdir)
            .output()
            .map_err(|e| format!("git status spawn failed: {e}"))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(format!("git status failed: {stderr}"));
        }
        let porcelain = String::from_utf8_lossy(&out.stdout);
        porcelain.lines().any(|l| !l.trim().is_empty())
    };

    // Recovery snapshot AFTER the dirtiness check so it reflects the state the
    // user expects to roll back to. Logs + swallows (mirrors `try_snapshot`).
    if let Err(e) = corvus_git::recovery::snapshot_with_policy(
        invoker,
        r,
        RecoveryKind::Pull,
        format!("before pull from '{remote}'"),
        policy,
    ) {
        eprintln!("corvus-be: recovery snapshot skipped: {e}");
    }

    // CLI stash_save with include_untracked=true: new files must go into the
    // stash too, else the SAFE checkout would abort the pull on any collision.
    let stash_entry: Option<StashEntry> = if is_dirty {
        emit_phase(progress, PullPhase::Stash, Some("workdir dirty — saving stash"));
        let entry = corvus_git::stash::stash_save(invoker, workdir, Some("arbor: pre-pull stash"), true)
            .map_err(|e| format!("stash failed: {e}"))?;
        Some(entry)
    } else {
        emit_phase_skipped(progress, PullPhase::Stash, Some("workdir clean — no stash"));
        None
    };

    // Pull — fetch + fast-forward / merge. We report Fetch then Merge as the two
    // phases the user mentally tracks (the crate's `pull` does both internally).
    emit_phase(progress, PullPhase::Fetch, Some(remote));
    let pull_result = corvus_git::remote::pull(invoker, r, remote, resolver);
    if pull_result.is_ok() {
        emit_phase(progress, PullPhase::Merge, None);
    }

    // Always re-apply (never pop) so the stash survives conflicts.
    let (stash_conflicts, pre_pull_stash, stash_apply_error) = if let Some(ref saved) = stash_entry {
        emit_phase(progress, PullPhase::Unstash, Some("re-applying"));
        match corvus_git::stash::stash_apply(invoker, r, 0) {
            Ok(res) if res.has_conflicts => {
                let entry = StashEntry { index: 0, message: saved.message.clone(), oid: saved.oid.clone() };
                (res.conflicted_files, Some(entry), None)
            }
            Ok(_) => {
                // Clean apply — drop the stash entry now that it's been restored.
                let _ = r.stash_drop(0);
                (vec![], None, None)
            }
            Err(e) => {
                // Non-conflict failure (e.g. locked index / antivirus on Windows):
                // preserve the stash reference so the user can re-apply manually.
                eprintln!("corvus-be: stash re-apply after pull failed: {e}");
                let entry = StashEntry { index: 0, message: saved.message.clone(), oid: saved.oid.clone() };
                (vec![], Some(entry), Some(e.to_string()))
            }
        }
    } else {
        emit_phase_skipped(progress, PullPhase::Unstash, Some("nothing to restore"));
        (vec![], None, None)
    };

    let pull_error = pull_result.err().map(|e| e.to_string());
    Ok(PullResult { stash_conflicts, pre_pull_stash, stash_apply_error, pull_error })
}
