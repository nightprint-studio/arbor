//! `ci` domain — CI-provider REST handlers (GitHub Actions + GitLab CI),
//! served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::ci`), but the context is [`CorvusState`] and the
//! provider comes from the reverse-channel registry ([`crate::provider`])
//! instead of the shell's `GitProviderRegistry`. The trait work is the shared
//! `corvus-git-provider-{api,github,gitlab}` crates, so results and
//! `ProviderError` wire strings are identical to in-process. Each network handler
//! resolves via [`provider_for_tab`](crate::provider::provider_for_tab) (briefly
//! locking, returning owned `Arc`s) so no `MutexGuard` is held across the
//! `.await` and the futures stay `Send`. **No hooks fire in this domain** — the
//! pipeline lifecycle hooks (`on_pipeline_started` / `on_pipeline_done` / …)
//! belong to the plugin-defined local pipeline runner, not to these provider
//! REST handlers.
//!
//! `get_ci_provider` runs here too: it opens the repo by the pushed path, lists
//! remotes, and runs the **pure** `CiProviderInfo::detect_from_remotes` (origin
//! preference + URL parsing, returning `Ok(None)` when no GitHub/GitLab remote
//! exists — unlike `provider_for_tab`, which errors). The only keyring-coupled
//! bit, `has_token`, is filled over the reverse channel (`__has_token`).
//!
//! **Left in-process** (not moved here):
//! - The **local pipeline engine** (run/resume/cancel/discard, registry readers)
//!   lives in the shell's `crate::ipc::corvus::pipeline`: it is the
//!   plugin-defined engine driven by an injected `PipelineRuntime` and belongs to
//!   a separate seam — out of scope here.

use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    CiFilter, CiJob, CiProviderInfo, CiRun, CiWorkflow, PipelineCreateRequest, ProviderError,
};
use serde_json::json;

use crate::provider::{maybe_refresh, pe, provider_for_tab};
use crate::repo::open;

/// Detect the active repo's CI provider (GitHub Actions / GitLab CI) from its
/// remotes, or `Ok(None)` when none match. The URL detection is pure
/// (`CiProviderInfo::detect_from_remotes`); only `has_token` — a keyring read —
/// crosses the reverse channel (`__has_token`), keeping the `Ok(None)` contract
/// byte-identical to the in-process copy.
#[arbor_rpc::handler]
fn get_ci_provider(state: &CorvusState, tab_id: String) -> Result<Option<CiProviderInfo>, String> {
    let repo = open(state, &tab_id)?;
    let remotes: Vec<(String, String)> = corvus_git::remote::list_remotes(&repo)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.name, r.url))
        .collect();

    let Some(mut info) = CiProviderInfo::detect_from_remotes(&remotes) else {
        return Ok(None);
    };
    // `has_token` is keyring-coupled (shell-side); fill it over the reverse
    // channel. A missing channel / probe failure leaves it false (no token).
    if let Some(host) = state.host_caller() {
        if let Ok(v) = host.call(
            "__has_token",
            json!({ "provider": info.provider, "gitlab_base_url": info.gitlab_base_url }),
        ) {
            info.has_token = serde_json::from_value(v).unwrap_or(false);
        }
    }
    Ok(Some(info))
}

/// Fetch the most recent CI runs for the active repo tab.
/// Calls the GitHub / GitLab REST API with the stored OAuth token.
#[arbor_rpc::handler]
async fn fetch_ci_runs(state: &CorvusState, tab_id: String) -> Result<Vec<CiRun>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved
        .provider
        .list_ci_runs(&resolved.repo, CiFilter::default())
        .await
        .map_err(pe)
}

/// Fetch the jobs / steps for a single CI run.
/// Returns a flat list of `CiJob`; the frontend groups them by `stage`.
#[arbor_rpc::handler]
async fn fetch_ci_jobs(
    state: &CorvusState,
    tab_id: String,
    run_id: String,
) -> Result<Vec<CiJob>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved
        .provider
        .fetch_ci_jobs(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}

/// List GitHub Actions workflows available in the repo (empty Vec for GitLab).
/// Used to populate the workflow picker in the "create pipeline" modal.
#[arbor_rpc::handler]
async fn list_ci_workflows(state: &CorvusState, tab_id: String) -> Result<Vec<CiWorkflow>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
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
#[arbor_rpc::handler]
async fn create_ci_pipeline(
    state: &CorvusState,
    tab_id: String,
    branch: String,
    variables: Vec<(String, String)>,
    workflow_id: Option<String>,
) -> Result<Option<String>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);

    if resolved.info.provider == "github" && workflow_id.is_none() {
        return Err("workflow_id is required for GitHub".into());
    }
    let req = PipelineCreateRequest { branch, variables, workflow_id };
    let run = resolved
        .provider
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
/// GitHub, **detached merge-request pipelines** on GitLab. The provider does the
/// MR-scoped aggregation internally (GitHub merges branch + head-sha runs;
/// GitLab merges MR-pipeline + branch runs), keyed off `mr_number` being set on
/// the filter. Results are merged, deduplicated by pipeline `id`, and sorted
/// newest-first.
#[arbor_rpc::handler]
async fn fetch_mr_ci_runs(
    state: &CorvusState,
    tab_id: String,
    mr_number: i64,
    source_branch: String,
    head_sha: Option<String>,
) -> Result<Vec<CiRun>, String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);

    let filter = CiFilter {
        branch:    Some(source_branch),
        mr_number: Some(mr_number as u64),
        head_sha:  head_sha.filter(|s| !s.is_empty()),
        ..Default::default()
    };
    resolved.provider.list_ci_runs(&resolved.repo, filter).await.map_err(pe)
}

/// Re-trigger (re-run) a CI run by its provider-native ID.
#[arbor_rpc::handler]
async fn retrigger_ci_run(
    state: &CorvusState,
    tab_id: String,
    run_id: String,
) -> Result<(), String> {
    let resolved = provider_for_tab(state, &tab_id)?;
    maybe_refresh(&resolved.info.provider);
    resolved
        .provider
        .retrigger_ci_run(&resolved.repo, &run_id)
        .await
        .map_err(pe)
}
