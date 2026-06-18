//! `ci` domain — CI-provider REST handlers (GitHub Actions + GitLab CI)
//! routed through the in-process broker.
//!
//! These are the network-bound CI readers/triggers: list runs, fetch jobs,
//! list workflows, create/retrigger pipelines, and MR-scoped run aggregation.
//! Each `async fn` registers as `Kind::Async` and `.await`s the provider's REST
//! round-trip. `provider_for_tab` locks briefly and returns owned `Arc`s, so no
//! `MutexGuard` is held across the `.await` and the futures stay `Send`.
//!
//! The **local pipeline engine** (run/resume/cancel/discard, registry readers)
//! stays inline in `commands/pipeline_commands.rs`: it is sync, drives the
//! orchestrator via `AppHandle`/emit, and belongs to a separate seam.

use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::{CiJob, CiProviderInfo, CiRun, CiWorkflow};
use crate::git_provider::{provider_for_tab, types::{CiFilter, PipelineCreateRequest}};
use crate::ipc::corvus;
use crate::AppState;

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

/// Detect the CI provider for the active repo tab.
/// Returns `None` when the repo has no GitHub/GitLab remote, or when no
/// remote could be determined (no tab open).
#[corvus::handler]
fn get_ci_provider(
    state:  &AppState,
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
#[corvus::handler]
async fn fetch_ci_runs(
    state:  &AppState,
    tab_id: String,
) -> Result<Vec<CiRun>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .list_ci_runs(&resolved.repo, CiFilter::default())
        .await
        .map_err(pe)
}

/// Fetch the jobs / steps for a single CI run.
/// Returns a flat list of `CiJob`; the frontend groups them by `stage`.
#[corvus::handler]
async fn fetch_ci_jobs(
    state:  &AppState,
    tab_id: String,
    run_id: String,
) -> Result<Vec<CiJob>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .fetch_ci_jobs(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}

/// List GitHub Actions workflows available in the repo (empty Vec for GitLab).
/// Used to populate the workflow picker in the "create pipeline" modal.
#[corvus::handler]
async fn list_ci_workflows(
    state:  &AppState,
    tab_id: String,
) -> Result<Vec<CiWorkflow>> {
    let resolved = provider_for_tab(state, &tab_id)?;
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
#[corvus::handler]
async fn create_ci_pipeline(
    state:       &AppState,
    tab_id:      String,
    branch:      String,
    variables:   Vec<(String, String)>,
    workflow_id: Option<String>,
) -> Result<Option<String>> {
    let resolved = provider_for_tab(state, &tab_id)?;
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
#[corvus::handler]
async fn fetch_mr_ci_runs(
    state:         &AppState,
    tab_id:        String,
    mr_number:     i64,
    source_branch: String,
    head_sha:      Option<String>,
) -> Result<Vec<CiRun>> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;

    // The provider does the MR-scoped aggregation internally (GitHub merges
    // branch + head-sha runs; GitLab merges MR-pipeline + branch runs), keyed
    // off `mr_number` being set on the filter.
    let filter = CiFilter {
        branch:    Some(source_branch),
        mr_number: Some(mr_number as u64),
        head_sha:  head_sha.filter(|s| !s.is_empty()),
        ..Default::default()
    };
    resolved.provider.list_ci_runs(&resolved.repo, filter).await.map_err(pe)
}

/// Re-trigger (re-run) a CI run by its provider-native ID.
#[corvus::handler]
async fn retrigger_ci_run(
    state:  &AppState,
    tab_id: String,
    run_id: String,
) -> Result<()> {
    let resolved = provider_for_tab(state, &tab_id)?;
    crate::auth::maybe_refresh_for_provider(&resolved.info.provider).await;
    resolved.provider
        .retrigger_ci_run(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}
