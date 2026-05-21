use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

use crate::error::AppError;
use crate::git::remote::{FetchResult, RemoteInfo};
use crate::git::stash::StashEntry;
use crate::process_ext::NoWindowExt;
use crate::AppState;

/// Returned by `pull_branch` so the frontend knows whether a pre-pull stash
/// needed to be re-applied and whether that re-apply had conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResult {
    /// Relative paths of files that conflicted when re-applying the stash.
    /// Empty when no stash was made, or when the apply was clean.
    pub stash_conflicts: Vec<String>,
    /// The stash entry that was created before pulling, if any.
    /// Present (and index = 0) when `stash_conflicts` is non-empty so the
    /// frontend can open the conflict-resolution modal.
    /// Also present when `stash_apply_error` is set so the user knows the
    /// stash can be recovered from the Stash panel (it is still at index 0).
    pub pre_pull_stash: Option<StashEntry>,
    /// Non-None when the stash re-apply failed for a non-conflict reason
    /// (e.g. file lock, antivirus scan holding the file open on Windows).
    /// The stash entry is still at index 0 — the user must apply it manually
    /// via the Stash panel or `git stash apply`.
    pub stash_apply_error: Option<String>,
    /// Non-None when the pull fetch/merge itself failed.
    /// Any stash context is still communicated via the other fields so the
    /// user can recover their work before retrying the pull.
    pub pull_error: Option<String>,
}

// ── Open-in-browser helpers ────────────────────────────────────────────────────
// URL transformations are centralised in git::url to avoid duplication with
// auth::credential_store and pipeline::ci_client.

use crate::git::url::{normalize_to_https, forge_url};

fn get_first_remote_url(repo: &git2::Repository) -> Result<String, AppError> {
    let remotes = repo.remotes().map_err(AppError::Git)?;
    let name = remotes
        .iter()
        .flatten()
        .find(|r| *r == "origin")
        .or_else(|| remotes.iter().flatten().next())
        .ok_or_else(|| AppError::Other("No remotes configured for this repository".into()))?
        .to_owned();
    let remote = repo.find_remote(&name).map_err(AppError::Git)?;
    remote
        .url()
        .ok_or_else(|| AppError::Other("Remote URL is not valid UTF-8".into()))
        .map(|s| s.to_string())
}

#[tauri::command]
pub fn open_in_browser(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    tab_id: String,
    target: String,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let url = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let remote_url = get_first_remote_url(repo.inner())?;
        let base = normalize_to_https(&remote_url).ok_or_else(|| {
            AppError::Other(format!("Cannot build browser URL for remote: {}", remote_url))
        })?;
        forge_url(&base, &target)
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| AppError::Other(format!("Failed to open browser: {}", e)))?;
    Ok(())
}

#[tauri::command]
pub fn list_remotes(
    state: State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<RemoteInfo>, AppError> {
    let mut mgr = state.lock_repos()?;
    let repo = mgr.get(&tab_id)?;
    crate::git::remote::list_remotes(repo.inner())
}

#[tauri::command]
pub async fn fetch_remote(
    state: State<'_, AppState>,
    tab_id: String,
    remote: String,
) -> Result<FetchResult, AppError> {
    // Grab the repo path (and remote URL for token refresh) under a brief lock,
    // then release the mutex before the network call so other git ops stay
    // responsive during slow fetch I/O.
    let (repo_path, remote_url) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let url = repo.inner()
            .find_remote(&remote)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()))
            .unwrap_or_default();
        (repo.path.clone(), url)
    };

    crate::auth::maybe_refresh_for_url(&remote_url).await;

    // Run the blocking fetch on tokio's blocking pool so the IPC thread (and
    // any other .await-ing Tauri command) stays responsive.
    let remote_for_task = remote.clone();
    let result = tokio::task::spawn_blocking(move || -> Result<FetchResult, AppError> {
        let repo = git2::Repository::open(&repo_path).map_err(AppError::Git)?;
        crate::git::remote::fetch(&repo, &remote_for_task)
    })
    .await
    .map_err(|e| AppError::Other(format!("fetch task panicked: {e}")))??;

    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({ "tab_id": &tab_id, "remote": &remote });
        let _ = host.fire_hook("on_fetch", &ctx.to_string());
    }
    Ok(result)
}

#[tauri::command]
pub async fn push_branch(
    state: State<'_, AppState>,
    tab_id: String,
    remote: String,
    refspec: String,
    force: bool,
) -> Result<(), AppError> {
    let (repo_path, remote_url) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let url = repo.inner()
            .find_remote(&remote)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()))
            .unwrap_or_default();
        (repo.path.clone(), url)
    };

    crate::auth::maybe_refresh_for_url(&remote_url).await;

    let remote_task  = remote.clone();
    let refspec_task = refspec.clone();
    tokio::task::spawn_blocking(move || -> Result<(), AppError> {
        let repo = git2::Repository::open(&repo_path).map_err(AppError::Git)?;
        crate::git::remote::push(&repo, &remote_task, &refspec_task, force)
    })
    .await
    .map_err(|e| AppError::Other(format!("push task panicked: {e}")))??;

    if let Ok(host) = state.lock_plugin_host() {
        let ctx = serde_json::json!({
            "tab_id":  &tab_id,
            "remote":  &remote,
            "refspec": &refspec,
            "force":   force,
        });
        let _ = host.fire_hook("on_push", &ctx.to_string());
    }
    Ok(())
}

/// `op_id`: optional opaque id correlated by the frontend OperationsOverlay.
/// When `Some`, the pull emits `arbor://pull-progress` events keyed by that
/// id plus a final `arbor://pull-done`.  `None` keeps the legacy silent
/// behaviour for callers that don't want overlay feedback.
#[tauri::command]
pub async fn pull_branch(
    app:    tauri::AppHandle,
    state:  State<'_, AppState>,
    tab_id: String,
    remote: String,
    op_id:  Option<String>,
) -> Result<PullResult, AppError> {
    // Extract repo path, workdir and remote URL up front, then release the
    // mutex before the network call.
    let (repo_path, workdir, remote_url) = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        let wd = repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf();
        let url = repo.inner()
            .find_remote(&remote)
            .ok()
            .and_then(|r| r.url().map(|s| s.to_string()))
            .unwrap_or_default();
        (repo.path.clone(), wd, url)
    };

    crate::auth::maybe_refresh_for_url(&remote_url).await;

    let tab_id_task = tab_id.clone();
    let remote_task = remote.clone();
    let progress    = op_id.clone().map(|oid| (app.clone(), oid));
    let result: Result<PullResult, AppError> = tokio::task::spawn_blocking(move || {
        let mut r = git2::Repository::open(&repo_path).map_err(AppError::Git)?;
        pull_branch_inner(&mut r, &workdir, &remote_task, progress.as_ref())
            .map(|pr| (pr, tab_id_task))
            .map(|(pr, _)| pr)
    })
    .await
    .map_err(|e| AppError::Other(format!("pull task panicked: {e}")))?;

    // Always emit pull-done so the OperationsOverlay finalises the card.
    if let Some(ref oid) = op_id {
        use tauri::Emitter;
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
                let _ = app.emit("arbor://pull-done", serde_json::json!({
                    "op_id":   oid,
                    "status":  status,
                    "error":   error,
                    "summary": serde_json::Value::Null,
                }));
            }
            Err(e) => {
                let _ = app.emit("arbor://pull-done", serde_json::json!({
                    "op_id":   oid,
                    "status":  "error",
                    "error":   e.to_string(),
                    "summary": serde_json::Value::Null,
                }));
            }
        }
    }

    let pr = result?;

    // Fire the on_pull hook only on clean success (no stash apply error,
    // no pull error, no conflicts).  Plugin host access is restored from the
    // AppHandle since the original `state` doesn't survive the await point
    // cleanly under all Tauri versions.
    if pr.pull_error.is_none() && pr.stash_apply_error.is_none() && pr.stash_conflicts.is_empty() {
        let state_post = app.state::<AppState>();
        if let Ok(host) = state_post.lock_plugin_host() {
            let ctx = serde_json::json!({ "tab_id": &tab_id, "remote": &remote });
            let _ = host.fire_hook("on_pull", &ctx.to_string());
        };
    }

    // If the pull failed AND there's no stash context to communicate, surface
    // it as an Err so the frontend toasts a simple error.  Otherwise the
    // PullResult carries everything the frontend needs to drive recovery UI.
    if let Some(err) = pr.pull_error.clone() {
        if pr.pre_pull_stash.is_none() && pr.stash_conflicts.is_empty() {
            return Err(AppError::Other(err));
        }
    }
    Ok(pr)
}

// ---------------------------------------------------------------------------
// Pull phases — emitted as `arbor://pull-progress` events when the caller
// passes an op_id.  Drives the OperationsOverlay ProgressStepper.
// ---------------------------------------------------------------------------

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

/// Side-table that lets pull_branch_inner emit progress without taking the
/// closure-juggling penalty.  When `None`, all `emit_*` calls are silent.
type PullProgress<'a> = Option<&'a (tauri::AppHandle, String)>;

fn emit_phase(progress: PullProgress<'_>, phase: PullPhase, detail: Option<&str>) {
    let Some((app, oid)) = progress else { return };
    use tauri::Emitter;
    let _ = app.emit("arbor://pull-progress", serde_json::json!({
        "op_id":   oid,
        "phase":   phase.key(),
        "label":   phase.label(),
        "detail":  detail,
        "skipped": false,
    }));
}

fn emit_phase_skipped(progress: PullProgress<'_>, phase: PullPhase, detail: Option<&str>) {
    let Some((app, oid)) = progress else { return };
    use tauri::Emitter;
    let _ = app.emit("arbor://pull-progress", serde_json::json!({
        "op_id":   oid,
        "phase":   phase.key(),
        "label":   phase.label(),
        "detail":  detail,
        "skipped": true,
    }));
}

/// Synchronous pull flow, extracted so it can run on tokio's blocking pool
/// without capturing any non-Send Tauri handles.  Returns a PullResult that
/// folds stash context + pull error into a single value.
///
/// `progress`, when non-None, is a (AppHandle, op_id) pair used to emit
/// `arbor://pull-progress` events as the flow advances.  The final
/// `arbor://pull-done` event is emitted by the outer async wrapper after
/// the result is assembled, since it depends on the PullResult fields
/// (stash_conflicts, stash_apply_error, pull_error) for status mapping.
fn pull_branch_inner(
    r: &mut git2::Repository,
    workdir: &std::path::Path,
    remote: &str,
    progress: PullProgress<'_>,
) -> Result<PullResult, AppError> {

    // Safe pull flow:
    //  0. Full-tree recovery snapshot so the user can roll back via the
    //     Recovery panel even if the stash + pull + re-apply chain goes
    //     sideways in a way we don't anticipate. Covers both tracked
    //     modifications AND untracked files.
    //  1. Stash local changes via git CLI with -u (INCLUDE_UNTRACKED so
    //     nothing is lost, and the reflog is written so other tools can
    //     see the stash).
    //  2. Pull (fast-forward uses the SAFE checkout that refuses to
    //     overwrite collisions — see git::remote::pull).
    //  3. Re-apply the stash with stash_apply (NOT stash_pop) so the stash
    //     entry is *always* preserved even when re-apply has conflicts.
    //  4. If there are conflicts, return them in PullResult — the frontend
    //     will open the StashConflictModal so the user can resolve them.
    tracing::info!(
        target: "pull",
        "pull_branch_inner ENTER remote={remote} workdir={}",
        workdir.display()
    );

    // ═════════════════════════════════════════════════════════════════════════
    // WHY WE USE `git status --porcelain` (CLI) HERE, NOT libgit2's statuses()
    // ═════════════════════════════════════════════════════════════════════════
    //
    // The symptom we hit: `r.statuses(...)` returns ZERO dirty entries, but
    // `git status --porcelain` from a shell correctly reports ` M <path>`.
    // Under the false-clean reading, stash_save was skipped and `git merge
    // --ff-only` then bailed with "local changes would be overwritten".
    //
    // The CRUCIAL detail: it's not that libgit2 is broken across the board.
    // The same user's UI shows the modification in the sidebar just fine —
    // and that's ALSO computed from `repo.statuses()`. So libgit2 DOES see
    // the change everywhere else in Arbor.
    //
    // What's different here is HOW the Repository handle is obtained:
    //
    //   · Every UI-facing status call uses the SHARED `Repository` stored
    //     in `state.repos` (opened once on repo-load, reused across all
    //     commands). By the time the user clicks Pull, that handle has
    //     serviced dozens of other `statuses()` calls — which have as a
    //     side effect kept libgit2's in-memory stat cache in sync with
    //     the workdir.
    //   · `pull_branch`, however, does network I/O (fetch), which means
    //     it cannot hold the `state.repos` mutex across the `.await`.
    //     Instead it clones the repo path out of the mutex and opens a
    //     FRESH `git2::Repository::open(&repo_path)` inside a
    //     `spawn_blocking` task (see line ~199 of this file).
    //   · A freshly-opened Repository loads `.git/index` from disk and
    //     has NO warmed cache. On Windows in particular, the first
    //     `statuses()` on a cold handle can return a false-clean result
    //     when the workdir file's `stat()` happens to match the stored
    //     index stat closely enough to skip the content-hash check — a
    //     racy-stat situation libgit2 handles less robustly than git CLI.
    //     The shared handle wouldn't hit this because its cache has been
    //     walked through the workdir already.
    //
    // So: the bug is specifically that a correctness-critical dirtiness
    // decision is made off a cold Repository handle. Options we
    // considered:
    //
    //   A. Share the UI Repository handle with pull (hold the mutex for
    //      the full pull including the network fetch). Rejected: serialises
    //      every other git operation behind a potentially-slow network
    //      call — visible UI stall on flaky connections.
    //   B. Warm the fresh Repository by running `index.read(true)` or by
    //      pre-reading each workdir file's stat before the `statuses`
    //      call. Brittle: relies on knowing libgit2's caching semantics
    //      better than its documentation spells out.
    //   C. Defer the decision to `git status --porcelain` via CLI.
    //      Canonical answer, no in-process state to be stale, matches
    //      exactly what the user sees from their shell. ~5ms subprocess
    //      cost on Windows — negligible for a network-bound operation.
    //
    // We chose (C). The broader pattern is also safer: "mutating /
    // correctness-critical decisions go through the git CLI; read-only
    // queries that feed UI can keep using libgit2 on the shared handle".
    // ═════════════════════════════════════════════════════════════════════════
    emit_phase(progress, PullPhase::Status, None);
    let is_dirty = {
        let out = crate::git_cli::command()
            .args(["status", "--porcelain"])
            .current_dir(workdir)
            .no_window()
            .output()
            .map_err(|e| AppError::Other(format!("git status spawn failed: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(AppError::Other(format!("git status failed: {stderr}")));
        }
        let porcelain = String::from_utf8_lossy(&out.stdout);
        let dirty_paths: Vec<String> = porcelain.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.to_string())
            .collect();
        let dirty = !dirty_paths.is_empty();
        tracing::info!(target: "pull", "is_dirty={dirty} (via CLI) paths={dirty_paths:?}");
        dirty
    };

    // Take the recovery snapshot AFTER the dirtiness check. Keeping it
    // after also means our snapshot reflects the state the user expects
    // to roll back to if anything in the pull chain misbehaves later.
    crate::git::recovery::try_snapshot(
        r,
        crate::git::recovery::RecoveryKind::Pull,
        format!("before pull from '{remote}'"),
    );

    // Use CLI stash_save so the reflog is written correctly on all platforms.
    // Pass include_untracked=true: new files that haven't been `git add`-ed
    // yet MUST go into the stash too, otherwise they're sitting exposed in
    // the workdir while the pull runs and the SAFE checkout would abort the
    // whole pull on any collision. Stashing them preserves them across the
    // round-trip and lets the apply restore them afterward.
    let stash_entry: Option<crate::git::stash::StashEntry> = if is_dirty {
        emit_phase(progress, PullPhase::Stash, Some("workdir dirty — saving stash"));
        let entry = crate::git::stash::stash_save(workdir, Some("arbor: pre-pull stash"), true)
            .map_err(|e| {
                tracing::error!(target: "pull", "stash_save FAILED: {e}");
                AppError::Other(format!("stash failed: {e}"))
            })?;
        tracing::info!(target: "pull", "stash_save OK oid={} msg={}", entry.oid, entry.message);
        // Verify via CLI (same reason we do dirtiness detection via CLI —
        // libgit2's stat cache isn't trustworthy here).
        if let Ok(out) = crate::git_cli::command()
            .args(["status", "--porcelain"])
            .current_dir(workdir)
            .no_window()
            .output()
        {
            let porcelain = String::from_utf8_lossy(&out.stdout);
            let still_dirty: Vec<String> = porcelain.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect();
            if !still_dirty.is_empty() {
                tracing::error!(
                    target: "pull",
                    "workdir STILL DIRTY after stash_save (oid={}) — paths={:?}",
                    entry.oid, still_dirty,
                );
            } else {
                tracing::info!(target: "pull", "workdir clean after stash_save");
            }
        }
        Some(entry)
    } else {
        emit_phase_skipped(progress, PullPhase::Stash, Some("workdir clean — no stash"));
        None
    };

    // Pull — fetch + fast-forward / merge.  We emit Fetch and Merge as
    // distinct steps even though git::remote::pull does both internally,
    // because they're the two phases the user mentally tracks.  Without
    // hooking into libgit2 progress callbacks we can't know exactly when
    // fetch ends and merge begins, so we report Fetch start before the
    // call and Merge start right after — close enough for UI feedback,
    // and on a clean repo the merge phase is sub-millisecond anyway.
    emit_phase(progress, PullPhase::Fetch, Some(remote));
    let pull_result = crate::git::remote::pull(r, remote);
    if pull_result.is_ok() {
        emit_phase(progress, PullPhase::Merge, None);
    }

    // Always re-apply (never pop) so the stash survives conflicts.
    let (stash_conflicts, pre_pull_stash, stash_apply_error) =
        if let Some(ref saved) = stash_entry {
            emit_phase(progress, PullPhase::Unstash, Some("re-applying"));
            let apply_result = crate::git::stash::stash_apply(r, 0);
            match apply_result {
                Ok(res) if res.has_conflicts => {
                    let entry = StashEntry {
                        index: 0,
                        message: saved.message.clone(),
                        oid: saved.oid.clone(),
                    };
                    (res.conflicted_files, Some(entry), None)
                }
                Ok(_) => {
                    // Clean apply — drop the stash entry now that it's been restored.
                    let _ = r.stash_drop(0);
                    (vec![], None, None)
                }
                Err(e) => {
                    // Apply failed for a non-conflict reason (e.g. locked index,
                    // antivirus holding the file open on Windows).
                    // Preserve the stash entry reference so the frontend can
                    // warn the user that their work is still in the stash list
                    // and needs to be applied manually.
                    tracing::warn!("stash re-apply after pull failed: {e}");
                    let entry = StashEntry {
                        index: 0,
                        message: saved.message.clone(),
                        oid: saved.oid.clone(),
                    };
                    (vec![], Some(entry), Some(e.to_string()))
                }
            }
        } else {
            emit_phase_skipped(progress, PullPhase::Unstash, Some("nothing to restore"));
            (vec![], None, None)
        };

    // Fold the pull outcome into a single result.  The outer async wrapper
    // decides whether to surface it as Err or Ok(PullResult) based on the
    // stash context fields.
    let pull_error = pull_result.err().map(|e| e.to_string());
    Ok(PullResult { stash_conflicts, pre_pull_stash, stash_apply_error, pull_error })
}
