use tauri::State;

use crate::AppState;
use crate::error::{AppError, Result};
use crate::git_provider::mr_impl::{
    CreateMrParams, MergeRequest, MergedMrHint, MrCapabilities, MrDetail, MrFeatureStatus,
    MrFileDiff, MrCommit,
    get_github_pr_commits,    get_gitlab_mr_commits,
    get_github_commit_files,  get_gitlab_commit_files,
    mark_github_pr_ready,     mark_gitlab_mr_ready,
    enable_github_auto_merge, enable_gitlab_auto_merge,
    disable_github_auto_merge, disable_gitlab_auto_merge,
    fetch_github_auto_merge_allowed, fetch_gitlab_mwps_supported,
    fetch_github_pr_feature_enabled, fetch_gitlab_mr_feature_enabled,
    wait_gitlab_merge_status_ready,
};
use crate::git_provider::{
    provider_for_tab, mr_id_from,
    types::{MrFilter, MergeOpts},
};

// ---------------------------------------------------------------------------
// ProviderError → AppError shim
// ---------------------------------------------------------------------------

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

// ---------------------------------------------------------------------------
// List MRs / PRs
// ---------------------------------------------------------------------------

/// List pull / merge requests for the active repo.
/// `state_filter`: "open" | "closed" | "merged" | "all"
#[tauri::command]
pub async fn list_mrs(
    state:        State<'_, AppState>,
    tab_id:       String,
    state_filter: Option<String>,
) -> Result<Vec<MergeRequest>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let filter = MrFilter {
        state: Some(state_filter.unwrap_or_else(|| "open".into())),
        ..Default::default()
    };
    resolved.provider.list_mrs(&resolved.repo, filter).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Get MR detail (with comments + checks)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_mr_detail(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<MrDetail> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.get_mr(&id).await.map_err(pe)
}

// ---------------------------------------------------------------------------
// Create MR / PR
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn create_mr(
    state:      State<'_, AppState>,
    app_handle: tauri::AppHandle,
    tab_id:     String,
    params:     CreateMrParams,
) -> Result<MergeRequest> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // Auto-merge requires the GitHub PR `node_id` (GraphQL) which the trait
    // surface does not expose yet — keep the direct path for that branch.
    // TODO(Phase 5): add `enable_auto_merge` to the trait.
    let auto = params.auto_merge;
    let squash = params.squash;
    let delete_branch = params.delete_branch;

    let mr = match resolved.info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::mr_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            let (mr, node_id) = crate::git_provider::mr_impl::create_github_pr(owner, repo, &params, &token).await?;

            if auto {
                let method = if squash { "SQUASH" } else { "MERGE" };
                match node_id {
                    Some(id) => match enable_github_auto_merge(&id, method, &token).await {
                        Ok(()) => emit_auto_merge_ok(&app_handle, mr.number),
                        Err(e) => emit_auto_merge_err(&app_handle, mr.number, &e.to_string()),
                    },
                    None => emit_auto_merge_err(
                        &app_handle,
                        mr.number,
                        "GitHub response missing node_id — cannot enable auto-merge.",
                    ),
                }
            }
            mr
        }
        "gitlab" => {
            let mr = resolved.provider
                .create_mr(&resolved.repo, params)
                .await
                .map_err(pe)?;

            if auto {
                let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
                let token = crate::git_provider::mr_impl::get_gitlab_token(base)?
                    .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
                let path  = resolved.info.project_path.as_deref().unwrap_or("");
                wait_gitlab_merge_status_ready(path, base, mr.number, &token).await;
                match enable_gitlab_auto_merge(
                    path, base, mr.number, squash, delete_branch, &token,
                ).await {
                    Ok(()) => emit_auto_merge_ok(&app_handle, mr.number),
                    Err(e) => emit_auto_merge_err(&app_handle, mr.number, &e.to_string()),
                }
            }
            mr
        }
        other => return Err(AppError::Other(format!("Unknown provider: {other}"))),
    };

    fire_mr_hook(&state, "on_mr_opened", &mr);
    Ok(mr)
}

fn emit_auto_merge_ok(app: &tauri::AppHandle, number: u64) {
    use tauri::Emitter;
    let _ = app.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Auto-merge armed for #{number}"),
        "message": "Will merge automatically once required checks pass.",
        "level":   "success",
    }));
}

fn emit_auto_merge_err(app: &tauri::AppHandle, number: u64, err: &str) {
    use tauri::Emitter;
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String = if short.len() > 240 { format!("{}…", &short[..240]) } else { short.to_string() };
    let _ = app.emit("plugin:notification", serde_json::json!({
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
#[tauri::command]
pub async fn get_mr_capabilities(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<MrCapabilities> {
    let resolved = match provider_for_tab(&state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrCapabilities::default()),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    match resolved.info.provider.as_str() {
        "github" => {
            let token = match crate::git_provider::mr_impl::get_github_token() {
                Ok(Some(t)) => t,
                _           => return Ok(MrCapabilities::default()),
            };
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            match fetch_github_auto_merge_allowed(owner, repo, &token).await {
                Ok(true)  => Ok(MrCapabilities::default()),
                Ok(false) => Ok(MrCapabilities {
                    auto_merge_supported: false,
                    auto_merge_reason:    Some(
                        "Auto-merge is disabled for this repository — \
                         enable it under Settings → General → Pull Requests on GitHub.".into(),
                    ),
                }),
                // Probe failed — stay permissive.
                Err(_) => Ok(MrCapabilities::default()),
            }
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = match crate::git_provider::mr_impl::get_gitlab_token(base) {
                Ok(Some(t)) => t,
                _           => return Ok(MrCapabilities::default()),
            };
            let path = resolved.info.project_path.as_deref().unwrap_or("");
            match fetch_gitlab_mwps_supported(path, base, &token).await {
                Ok(true)  => Ok(MrCapabilities::default()),
                Ok(false) => Ok(MrCapabilities {
                    auto_merge_supported: false,
                    auto_merge_reason:    Some(
                        "CI jobs are disabled for this project — there is no \
                         pipeline to wait on, so merge-when-pipeline-succeeds \
                         cannot be armed.".into(),
                    ),
                }),
                Err(_) => Ok(MrCapabilities::default()),
            }
        }
        _ => Ok(MrCapabilities::default()),
    }
}

// ---------------------------------------------------------------------------
// MR/PR feature probe (drives sidebar EmptyState + Palette gating)
// ---------------------------------------------------------------------------

/// Probe whether the active repo accepts merge/pull requests at all.
/// Permissive on failure: any missing token or network error returns
/// `enabled = true` so the user can still try.  Only an explicit signal
/// from the provider (archived/disabled repo on GitHub, MR access level
/// "disabled" on GitLab) flips it to `false`.
#[tauri::command]
pub async fn probe_mr_feature(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<MrFeatureStatus> {
    let resolved = match provider_for_tab(&state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(MrFeatureStatus::default()),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    match resolved.info.provider.as_str() {
        "github" => {
            let token = match crate::git_provider::mr_impl::get_github_token() {
                Ok(Some(t)) => t,
                _           => return Ok(MrFeatureStatus::default()),
            };
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            Ok(fetch_github_pr_feature_enabled(owner, repo, &token).await
                .unwrap_or_default())
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = match crate::git_provider::mr_impl::get_gitlab_token(base) {
                Ok(Some(t)) => t,
                _           => return Ok(MrFeatureStatus::default()),
            };
            let path  = resolved.info.project_path.as_deref().unwrap_or("");
            Ok(fetch_gitlab_mr_feature_enabled(path, base, &token).await
                .unwrap_or_default())
        }
        _ => Ok(MrFeatureStatus::default()),
    }
}

// ---------------------------------------------------------------------------
// Disable auto-merge
// ---------------------------------------------------------------------------

/// Cancel a previously-armed auto-merge / merge-when-pipeline-succeeds.
/// Idempotent: succeeds silently if auto-merge wasn't active.
#[tauri::command]
pub async fn disable_mr_auto_merge(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    match resolved.info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::mr_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            disable_github_auto_merge(owner, repo, number, &token).await?;
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = crate::git_provider::mr_impl::get_gitlab_token(base)?
                .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
            let path = resolved.info.project_path.as_deref().unwrap_or("");
            disable_gitlab_auto_merge(path, base, number, &token).await?;
        }
        other => return Err(AppError::Other(format!("Unknown provider: {other}"))),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Merge MR / PR
// ---------------------------------------------------------------------------

/// `source_branch` is required when `delete_branch = true` (GitHub needs it for the branch name).
#[tauri::command]
pub async fn merge_mr(
    state:         State<'_, AppState>,
    app_handle:    tauri::AppHandle,
    tab_id:        String,
    number:        u64,
    merge_method:  Option<String>,
    squash:        Option<bool>,
    delete_branch: Option<bool>,
    source_branch: Option<String>,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
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

    // GitHub: delete remote branch via `git push :refs/heads/<branch>` because
    // the REST DELETE endpoint has been observed to silently no-op under some
    // OAuth/App configurations.  GitLab handles delete server-side via
    // `merge_gitlab_mr` (already passed `do_delete`).
    if resolved.info.provider == "github" && do_delete {
        match source_branch.as_deref() {
            Some(branch) => delete_remote_via_push(
                &state, &app_handle, &tab_id, &resolved.info.remote_url,
                number, branch,
            ),
            None => emit_remote_delete_warning(
                &app_handle,
                number,
                "?",
                "no source branch was provided — cannot delete remote branch.",
            ),
        }
    }

    fire_mr_hook_by_number(&state, "on_mr_merged", number, &resolved.info.provider);
    Ok(())
}

fn emit_remote_delete_warning(app: &tauri::AppHandle, number: u64, branch: &str, err: &str) {
    use tauri::Emitter;
    let short = err.lines().next().unwrap_or(err);
    let trimmed: String = if short.len() > 400 { format!("{}…", &short[..400]) } else { short.to_string() };
    let _ = app.emit("plugin:notification", serde_json::json!({
        "plugin":  "arbor",
        "title":   format!("Remote branch '{branch}' not deleted after merging #{number}"),
        "message": trimmed,
        "level":   "warning",
    }));
}

/// Delete `<branch>` on the remote whose URL is `remote_url` using
/// `git push :refs/heads/<branch>`.  This is the same path the sidebar
/// "Delete remote branch" action uses; it relies on the repo's stored
/// git credentials rather than the provider's REST API token.
fn delete_remote_via_push(
    state:        &AppState,
    app_handle:   &tauri::AppHandle,
    tab_id:       &str,
    remote_url:   &str,
    number:       u64,
    branch:       &str,
) {
    let push_outcome: Result<Vec<String>> = (|| {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(tab_id)?;
        let remote_name = crate::git::remote::list_remotes(repo.inner())?
            .into_iter()
            .find(|r| r.url == remote_url)
            .map(|r| r.name)
            .unwrap_or_else(|| "origin".into());
        let qualified = format!("{remote_name}/{branch}");
        Ok(crate::git::branch::delete_remote_branches(repo.inner(), &[qualified]))
    })();
    match push_outcome {
        Ok(failed) if failed.is_empty() => {} // success → silent; sidebar refresh shows the branch is gone
        Ok(_) => emit_remote_delete_warning(
            app_handle, number, branch,
            "git push --delete failed (check the remote and your credentials)",
        ),
        Err(e) => emit_remote_delete_warning(
            app_handle, number, branch, &e.to_string(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Close / Reopen MR / PR
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn close_mr(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.close_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(&state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

#[tauri::command]
pub async fn reopen_mr(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.reopen_mr(&id).await.map_err(pe)?;
    fire_mr_hook_by_number(&state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mark as ready for review (remove draft status) — not yet on the trait
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn mark_mr_ready(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // TODO(Phase 5): add `mark_mr_ready` to the trait.
    match resolved.info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::mr_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            mark_github_pr_ready(owner, repo, number, &token).await?;
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = crate::git_provider::mr_impl::get_gitlab_token(base)?
                .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
            let path  = resolved.info.project_path.as_deref().unwrap_or("");
            mark_gitlab_mr_ready(path, base, number, &token).await?;
        }
        other => return Err(AppError::Other(format!("Unknown provider: {other}"))),
    }
    fire_mr_hook_by_number(&state, "on_mr_updated", number, &resolved.info.provider);
    Ok(())
}

// ---------------------------------------------------------------------------
// Add comment
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn add_mr_comment(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
    body:   String,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.add_mr_comment(&id, &body).await.map_err(pe)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// File diffs / commits
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_mr_files(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrFileDiff>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let id = mr_id_from(&resolved, number);
    resolved.provider.list_mr_files(&id).await.map_err(pe)
}

#[tauri::command]
pub async fn get_mr_commits(
    state:  State<'_, AppState>,
    tab_id: String,
    number: u64,
) -> Result<Vec<MrCommit>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    // TODO(Phase 5): add `list_mr_commits` to the trait.
    match resolved.info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::mr_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            get_github_pr_commits(owner, repo, number, &token).await
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = crate::git_provider::mr_impl::get_gitlab_token(base)?
                .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
            let path  = resolved.info.project_path.as_deref().unwrap_or("");
            get_gitlab_mr_commits(path, base, number, &token).await
        }
        other => Err(AppError::Other(format!("Unknown provider: {other}"))),
    }
}

#[tauri::command]
pub async fn get_mr_commit_diff(
    state:  State<'_, AppState>,
    tab_id: String,
    sha:    String,
) -> Result<Vec<MrFileDiff>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    // TODO(Phase 5): add `get_commit_diff` to the trait.
    match resolved.info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::mr_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = resolved.info.owner.as_deref().unwrap_or("");
            let repo  = resolved.info.repo_name.as_deref().unwrap_or("");
            get_github_commit_files(owner, repo, &sha, &token).await
        }
        "gitlab" => {
            let base  = resolved.info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = crate::git_provider::mr_impl::get_gitlab_token(base)?
                .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
            let path  = resolved.info.project_path.as_deref().unwrap_or("");
            get_gitlab_commit_files(path, base, &sha, &token).await
        }
        other => Err(AppError::Other(format!("Unknown provider: {other}"))),
    }
}

// ---------------------------------------------------------------------------
// Start MR conflict resolution
// ---------------------------------------------------------------------------

/// Prepare the local workspace to resolve a pull/merge-request conflict.
///
/// This command spawns a background job (visible in the JobsOverlay) that runs
/// the multi-step prep flow without blocking the Tauri runtime.  Returns the
/// `job_id` immediately.  Progress is reported via two custom events:
///
/// - `arbor://mr-conflict-progress` — `{ job_id, phase, phase_index,
///   phase_total, label, detail? }`.  Drives the ProgressStepper widget.
/// - `arbor://mr-conflict-done`     — `{ job_id, status: "clean" |
///   "conflicts" | "error", error? }`.  Triggers the success / open-resolver /
///   error path on the frontend.
///
/// The job also emits the standard `arbor://job-started`, `arbor://job-output`
/// and `arbor://job-done` events so per-line stdout/stderr appears in the
/// Job Output panel.
#[tauri::command]
pub fn mr_start_conflict_resolution(
    state:         State<'_, AppState>,
    app_handle:    tauri::AppHandle,
    tab_id:        String,
    source_branch: String,
    target_branch: String,
) -> Result<String> {
    use tauri::{Emitter, Manager};
    use crate::jobs::{JobInfo, JobRegistry, JobStatus};
    use crate::git::merge::{
        prepare_mr_conflict_resolution, MrPrepEvent, MrPrepOutcome, MrPrepPhase,
    };

    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        repo.inner()
            .workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };

    let name = format!("Resolve conflicts: {source_branch} ← {target_branch}");
    let job_id = {
        let mut jobs = state.lock_jobs()?;
        let id = jobs.new_id();
        jobs.register(JobInfo {
            id:              id.clone(),
            name:            name.clone(),
            plugin_name:     "arbor".to_string(),
            command:         format!("git fetch + checkout {source_branch} + merge origin/{target_branch}"),
            started_at:      JobRegistry::now_secs(),
            status:          JobStatus::Running,
            category:        Some("Merge".to_string()),
            non_cancellable: true,
            is_system:       false,
            finished_at:     None,
            hidden:          false,
        });
        id
    };

    let _ = app_handle.emit("arbor://job-started", serde_json::json!({
        "job_id":      &job_id,
        "name":        &name,
        "plugin_name": "arbor",
        "command":     format!("git fetch + checkout {source_branch} + merge origin/{target_branch}"),
        "category":    "Merge",
    }));

    let jid    = job_id.clone();
    let handle = app_handle.clone();
    let _ = std::thread::Builder::new()
        .name(format!("arbor-mr-conflict-{}", jid))
        .spawn(move || {
            let result = prepare_mr_conflict_resolution(
                &workdir,
                &source_branch,
                &target_branch,
                |evt| match evt {
                    MrPrepEvent::PhaseStart { phase, detail } => {
                        let _ = handle.emit("arbor://mr-conflict-progress", serde_json::json!({
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
                        if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                            jobs.append_output(&jid, header.clone());
                        }
                        let _ = handle.emit("arbor://job-output", serde_json::json!({
                            "job_id": &jid, "text": header,
                        }));
                    }
                    MrPrepEvent::Output { phase: _, line } => {
                        if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                            jobs.append_output(&jid, line.to_string());
                        }
                        let _ = handle.emit("arbor://job-output", serde_json::json!({
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

            if let Ok(mut jobs) = handle.state::<crate::AppState>().jobs.lock() {
                let s = match status_payload {
                    Ok(c)  => JobStatus::Completed { exit_code: c },
                    Err(_) => JobStatus::Failed { error: error_msg.clone().unwrap_or_default() },
                };
                jobs.set_status(&jid, s);
            }

            let _ = handle.emit("arbor://job-done", serde_json::json!({
                "job_id":    &jid,
                "success":   matches!(status_payload, Ok(_)),
                "exit_code": status_payload.unwrap_or(-1),
            }));

            let _ = handle.emit("arbor://mr-conflict-done", serde_json::json!({
                "job_id": &jid,
                "status": outcome_label,
                "error":  error_msg,
            }));
        });

    Ok(job_id)
}

// ---------------------------------------------------------------------------
// Squash-merge hints for graph visualization
// ---------------------------------------------------------------------------

/// Returns a list of `{ sourceBranch, mergeCommitSha }` for all merged
/// PRs/MRs.  Used by the graph to draw accurate ghost edges for squash merges.
/// Returns an empty Vec (never errors) when the provider is not configured or
/// the token is missing — the graph simply shows no ghost edges in that case.
#[tauri::command]
pub async fn get_merged_mr_hints(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<MergedMrHint>> {
    let resolved = match provider_for_tab(&state, &tab_id) {
        Ok(r)  => r,
        Err(_) => return Ok(vec![]),
    };
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

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
// Hook firing helpers
// ---------------------------------------------------------------------------

fn fire_mr_hook(
    state: &AppState,
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
    fire_hook(state, hook, ctx);
}

fn fire_mr_hook_by_number(
    state:    &AppState,
    hook:     &str,
    number:   u64,
    provider: &str,
) {
    let ctx = serde_json::json!({ "number": number, "provider": provider });
    fire_hook(state, hook, ctx);
}

fn fire_hook(state: &AppState, hook: &str, ctx: serde_json::Value) {
    if let Ok(host) = state.plugin_host.lock() {
        let ctx_str = ctx.to_string();
        let _ = host.fire_hook(hook, &ctx_str);
    }
}

