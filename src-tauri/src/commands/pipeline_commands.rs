use tauri::State;

use crate::AppState;
use crate::error::{AppError, Result};
use crate::pipeline::{PipelineDef, PipelineRun};
use crate::git_provider::ci_impl::{CiProviderInfo, CiRun, CiJob, CiWorkflow};
use crate::git_provider::{provider_for_tab, types::{CiFilter, PipelineCreateRequest}};

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

// ---------------------------------------------------------------------------
// Query commands
// ---------------------------------------------------------------------------

/// List all pipeline definitions registered by plugins.
#[tauri::command]
pub fn list_pipeline_defs(state: State<AppState>) -> Result<Vec<PipelineDef>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.defs.clone())
}

/// List all pipeline runs (most recent last).
#[tauri::command]
pub fn list_pipeline_runs(state: State<AppState>) -> Result<Vec<PipelineRun>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.runs.iter().rev().cloned().collect())
}

/// Get a single pipeline run by ID.
#[tauri::command]
pub fn get_pipeline_run(state: State<AppState>, run_id: String) -> Result<PipelineRun> {
    let reg = state.lock_pipelines()?;
    reg.get_run(&run_id)
        .cloned()
        .ok_or_else(|| AppError::Other(format!("pipeline run '{run_id}' not found")))
}

// ---------------------------------------------------------------------------
// Execution commands
// ---------------------------------------------------------------------------

/// Start a pipeline run. Returns the run ID.
/// `tab_id` is used to look up the active repo's working directory.
#[tauri::command]
pub fn run_pipeline(
    state:      State<AppState>,
    app_handle: tauri::AppHandle,
    plugin:     String,
    pipeline_id: String,
    tab_id:     Option<String>,
) -> Result<String> {
    // Find the definition.
    let def = {
        let reg = state.lock_pipelines()?;
        reg.defs.iter()
            .find(|d| d.id == pipeline_id && d.plugin == plugin)
            .cloned()
            .ok_or_else(|| AppError::Other(
                format!("pipeline '{pipeline_id}' not found in plugin '{plugin}'")
            ))?
    };

    // Resolve repo path for cwd fallback.
    let repo_path = tab_id.as_deref().and_then(|tid| {
        state.repos.lock().ok().and_then(|mut mgr| {
            mgr.get(tid).ok().map(|r| r.path.clone())
        })
    });

    // Build initial run state (all steps Pending) seeded with lock_key + log_level.
    let run_id = {
        let mut reg = state.lock_pipelines()?;
        reg.new_run_id()
    };
    let run = def.new_run(run_id.clone(), repo_path.clone());

    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut reg = state.lock_pipelines()?;
        reg.add_run(run, cancel.clone());
    }

    // Start the orchestrator thread.
    crate::pipeline::start_pipeline_run(def, run_id.clone(), repo_path, cancel, app_handle);

    Ok(run_id)
}

/// Request a pipeline run for a def the user picked from the panel.
///
/// A pipeline def is *self-contained* when its `stages` array is non-empty:
/// every step has its command / op / cwd already resolved (variable
/// substitution baked in) so the orchestrator can replay it without any
/// plugin context. We run those directly — same as `run_pipeline` — so a
/// def compiled by a previous run (sequence, combo button, …) keeps
/// working from the panel even when the user has switched tabs and the
/// owning plugin would not be able to recompute the context.
///
/// A def with empty `stages` is a *stub* the plugin registered upfront so
/// the panel has something to show. Stubs cannot be replayed verbatim — we
/// delegate to the plugin's `on_pipeline_run_request` hook so it can
/// materialise stages (typically by re-compiling a profile or run config)
/// and call `arbor.pipeline.run` itself. If the plugin has no such hook,
/// we surface a clear error rather than spawning a 0-step ghost run.
#[tauri::command]
pub fn request_pipeline_run(
    state:      State<AppState>,
    app_handle: tauri::AppHandle,
    plugin:     String,
    pipeline_id: String,
    tab_id:     Option<String>,
) -> Result<Option<String>> {
    // Look up the def to decide which path to take. Cloning is cheap (a
    // pipeline def is just a few stages of metadata) and we want to drop
    // the registry lock before firing into Lua.
    let def_stages_empty = {
        let reg = state.lock_pipelines()?;
        reg.defs.iter()
            .find(|d| d.id == pipeline_id && d.plugin == plugin)
            .map(|d| d.stages.is_empty())
    };

    match def_stages_empty {
        // Def found, has stages → run directly (self-contained).
        Some(false) => {
            let run_id = run_pipeline(state, app_handle, plugin, pipeline_id, tab_id)?;
            Ok(Some(run_id))
        }
        // Def found, but stages are empty → must route through the plugin.
        Some(true) => {
            let host = state.lock_plugin_host()?;
            if host.plugin_has_handler(&plugin, "on_pipeline_run_request") {
                let ctx = serde_json::json!({
                    "pipeline_id": pipeline_id,
                    "tab_id":      tab_id,
                }).to_string();
                host.fire_hook_on(&plugin, "on_pipeline_run_request", &ctx)?;
                Ok(None)
            } else {
                Err(AppError::Other(format!(
                    "pipeline '{pipeline_id}' is a placeholder — its owning plugin '{plugin}' has no `on_pipeline_run_request` hook to compile it. Launch it from the plugin's own UI first."
                )))
            }
        }
        // Def not found at all → standard not-found error from run_pipeline.
        None => {
            let run_id = run_pipeline(state, app_handle, plugin, pipeline_id, tab_id)?;
            Ok(Some(run_id))
        }
    }
}

/// Cancel a running pipeline (stops after the current step completes).
/// Also wakes any orchestrator parked on the global concurrency condvar so
/// a queued run's cancel takes effect within microseconds rather than
/// waiting out the 250 ms poll timeout.
#[tauri::command]
pub fn cancel_pipeline_run(state: State<AppState>, run_id: String) -> Result<()> {
    {
        let mut reg = state.lock_pipelines()?;
        reg.cancel(&run_id);
    }
    state.pipeline_cv.notify_all();
    Ok(())
}

/// Resume a failed/paused pipeline run from the step(s) that halted it.
#[tauri::command]
pub fn resume_pipeline_run(app_handle: tauri::AppHandle, run_id: String) -> Result<()> {
    crate::pipeline::resume_run(&run_id, app_handle).map_err(AppError::Other)
}

/// Drop a terminal (failed/cancelled/success) run from the registry and disk.
#[tauri::command]
pub fn discard_pipeline_run(app_handle: tauri::AppHandle, run_id: String) -> Result<()> {
    crate::pipeline::discard_run(&run_id, app_handle).map_err(AppError::Other)
}

/// Return the run_id currently holding `lock_key`, or `None` when the lock
/// is free. Used by plugins/UI to pre-flight "can I start?" checks.
#[tauri::command]
pub fn is_pipeline_locked(state: State<AppState>, lock_key: String) -> Result<Option<String>> {
    let reg = state.lock_pipelines()?;
    Ok(reg.locked_by(&lock_key).map(String::from))
}

// ---------------------------------------------------------------------------
// CI/CD integration (GitHub Actions + GitLab CI)
// ---------------------------------------------------------------------------

/// Detect the CI provider for the active repo tab.
/// Returns `None` when the repo has no GitHub/GitLab remote, or when no
/// remote could be determined (no tab open).
#[tauri::command]
pub fn get_ci_provider(
    state:  State<AppState>,
    tab_id: String,
) -> Result<Option<CiProviderInfo>> {
    let remotes: Vec<(String, String)> = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        crate::git::remote::list_remotes(repo.inner())?
            .into_iter()
            .map(|r| (r.name, r.url))
            .collect()
    };
    Ok(crate::git_provider::ci_impl::detect_from_remotes(&remotes))
}

/// Fetch the most recent CI runs for the active repo tab.
/// Calls the GitHub / GitLab REST API with the stored OAuth token.
#[tauri::command]
pub async fn fetch_ci_runs(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<CiRun>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .list_ci_runs(&resolved.repo, CiFilter::default())
        .await
        .map_err(pe)
}

/// Fetch the jobs / steps for a single CI run.
/// Returns a flat list of `CiJob`; the frontend groups them by `stage`.
#[tauri::command]
pub async fn fetch_ci_jobs(
    state:  State<'_, AppState>,
    tab_id: String,
    run_id: String,
) -> Result<Vec<CiJob>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .fetch_ci_jobs(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}

/// List GitHub Actions workflows available in the repo (empty Vec for GitLab).
/// Used to populate the workflow picker in the "create pipeline" modal.
#[tauri::command]
pub async fn list_ci_workflows(
    state:  State<'_, AppState>,
    tab_id: String,
) -> Result<Vec<CiWorkflow>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    use crate::git_provider::types::error::ProviderError;
    match resolved.provider.list_ci_workflows(&resolved.repo).await {
        Ok(v) => Ok(v),
        // GitLab pipelines aren't named workflows; treat as empty list to
        // preserve the legacy contract with the frontend.
        Err(ProviderError::Unsupported { .. }) => Ok(Vec::new()),
        Err(e) => Err(pe(e)),
    }
}

/// Create (trigger) a new CI pipeline run.
///
/// - GitLab: `POST /api/v4/projects/{id}/pipeline` — returns the new pipeline ID.
/// - GitHub:  `POST /actions/workflows/{workflow_id}/dispatches` — returns `None`
///            (GitHub does not expose the run ID synchronously; the caller should
///            refresh the run list after a short delay).
#[tauri::command]
pub async fn create_ci_pipeline(
    state:       State<'_, AppState>,
    tab_id:      String,
    branch:      String,
    variables:   Vec<(String, String)>,
    workflow_id: Option<String>,
) -> Result<Option<String>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    if resolved.info.provider == "github" && workflow_id.is_none() {
        return Err(AppError::Other("workflow_id is required for GitHub".into()));
    }
    let req = PipelineCreateRequest { branch, variables, workflow_id };
    let run = resolved.provider
        .create_ci_pipeline(&resolved.repo, req)
        .await
        .map_err(pe)?;
    // GitHub workflow_dispatch returns no run id synchronously — the trait
    // signals that with an empty `id`. GitLab fills it in.
    Ok(if run.id.is_empty() { None } else { Some(run.id) })
}

/// Fetch CI runs scoped to a single Merge Request / Pull Request.
///
/// Both providers can attach pipeline runs to a PR/MR via paths a plain
/// branch filter would miss — fork PRs and `pull_request_target` runs on
/// GitHub, **detached merge-request pipelines** on GitLab. To catch all of
/// them we hit two endpoints per provider in parallel and dedupe by id.
///
/// - **GitHub**:
///   - `/actions/runs?branch={source_branch}` — push and `pull_request` runs
///     whose `head_branch` matches.
///   - `/actions/runs?head_sha={head_sha}` — runs targeting the PR head SHA
///     directly (fork PRs, `pull_request_target`, manual workflow_dispatch
///     pinned to the SHA).
/// - **GitLab**:
///   - `/merge_requests/:iid/pipelines` — required for pipelines whose `ref`
///     is `refs/merge-requests/{iid}/head` (the "Merge request pipeline" rows
///     GitLab shows at the top of the MR page).
///   - `/pipelines?ref={source_branch}` — branch pipelines from pushes.
///
/// Results are merged, deduplicated by pipeline `id`, and sorted newest-first.
#[tauri::command]
pub async fn fetch_mr_ci_runs(
    state:         State<'_, AppState>,
    tab_id:        String,
    mr_number:     i64,
    source_branch: String,
    head_sha:      Option<String>,
) -> Result<Vec<CiRun>> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    let info = &resolved.info;

    // TODO(Phase 5): the trait only exposes a single `list_ci_runs(branch)` —
    // surface the parallel MR/SHA queries on the trait so this command can
    // route through the provider too.
    match info.provider.as_str() {
        "github" => {
            let token = crate::git_provider::ci_impl::get_github_token()?
                .ok_or_else(|| AppError::Other("GitHub token not found".into()))?;
            let owner = info.owner.as_deref().unwrap_or("");
            let repo  = info.repo_name.as_deref().unwrap_or("");

            let by_branch_fut = crate::git_provider::ci_impl::fetch_github_runs_for_branch(
                owner, repo, &source_branch, &token,
            );
            let by_sha_fut = async {
                if let Some(sha) = head_sha.as_deref().filter(|s| !s.is_empty()) {
                    crate::git_provider::ci_impl::fetch_github_runs_for_sha(
                        owner, repo, sha, &token,
                    ).await
                } else {
                    Ok(Vec::new())
                }
            };
            let (branch_res, sha_res) = tokio::join!(by_branch_fut, by_sha_fut);

            // The branch query is the primary source; surface its error.
            // The head-sha query is best-effort and may fail silently.
            let mut runs = branch_res?;
            if let Ok(sha_runs) = sha_res {
                let seen: std::collections::HashSet<String> =
                    runs.iter().map(|r| r.id.clone()).collect();
                for r in sha_runs {
                    if !seen.contains(&r.id) {
                        runs.push(r);
                    }
                }
            }
            // Newest first by run id (numeric, descending).
            runs.sort_by(|a, b| {
                let ai = a.id.parse::<i64>().unwrap_or(0);
                let bi = b.id.parse::<i64>().unwrap_or(0);
                bi.cmp(&ai)
            });
            Ok(runs)
        }
        "gitlab" => {
            let base  = info.gitlab_base_url.as_deref().unwrap_or("https://gitlab.com");
            let token = crate::git_provider::ci_impl::get_gitlab_token(base)?
                .ok_or_else(|| AppError::Other("GitLab token not found".into()))?;
            let path  = info.project_path.as_deref().unwrap_or("");

            let mr_pipelines_fut = crate::git_provider::ci_impl::fetch_gitlab_mr_pipelines(
                path, base, mr_number, &token,
            );
            let branch_pipelines_fut = crate::git_provider::ci_impl::fetch_gitlab_pipelines(
                path, base, &token,
            );
            let (mr_res, branch_res) = tokio::join!(mr_pipelines_fut, branch_pipelines_fut);

            // MR endpoint is the authoritative source; if it fails, surface the error.
            let mut runs = mr_res?;
            if let Ok(branch_runs) = branch_res {
                let seen: std::collections::HashSet<String> =
                    runs.iter().map(|r| r.id.clone()).collect();
                for r in branch_runs {
                    if r.branch == source_branch && !seen.contains(&r.id) {
                        runs.push(r);
                    }
                }
            }
            // Newest first by pipeline id (numeric, descending).
            runs.sort_by(|a, b| {
                let ai = a.id.parse::<i64>().unwrap_or(0);
                let bi = b.id.parse::<i64>().unwrap_or(0);
                bi.cmp(&ai)
            });
            Ok(runs)
        }
        other => Err(AppError::Other(format!("unknown CI provider: {other}"))),
    }
}

/// Re-trigger (re-run) a CI run by its provider-native ID.
#[tauri::command]
pub async fn retrigger_ci_run(
    state:  State<'_, AppState>,
    tab_id: String,
    run_id: String,
) -> Result<()> {
    let resolved = provider_for_tab(&state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .retrigger_ci_run(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}
