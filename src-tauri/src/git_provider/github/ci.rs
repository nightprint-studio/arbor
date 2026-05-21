//! GitHub Actions (CI) operations.
//!
//! Phase 2 transition: every function delegates to the original
//! `crate::git_provider::ci_impl::*` implementation.

use crate::git_provider::types::{
    CiRun, CiJob, CiWorkflow, CiFilter, PipelineCreateRequest, RepoRef,
    error::ProviderError,
};

use super::api;

fn token() -> Result<String, ProviderError> {
    api::get_token()
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn repo_parts<'a>(repo: &'a RepoRef) -> Result<(&'a str, &'a str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name  = repo.name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub RepoRef requires name".into())
    })?;
    Ok((owner, name))
}

pub async fn list_ci_runs(repo: &RepoRef, filter: CiFilter) -> Result<Vec<CiRun>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    if let Some(branch) = filter.branch.as_deref() {
        crate::git_provider::ci_impl::fetch_github_runs_for_branch(owner, name, branch, &token)
            .await
            .map_err(ProviderError::from)
    } else {
        crate::git_provider::ci_impl::fetch_github_runs(owner, name, &token)
            .await
            .map_err(ProviderError::from)
    }
}

pub async fn get_ci_run(_repo: &RepoRef, _run_id: &str) -> Result<CiRun, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_ci_run".into() })
}

pub async fn fetch_ci_jobs(repo: &RepoRef, run_id: &str) -> Result<Vec<CiJob>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    crate::git_provider::ci_impl::fetch_github_jobs(owner, name, run_id, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_ci_job_log(_repo: &RepoRef, _job_id: &str) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_ci_job_log".into() })
}

pub async fn retrigger_ci_run(repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    crate::git_provider::ci_impl::retrigger_github_run(owner, name, run_id, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn cancel_ci_run(_repo: &RepoRef, _run_id: &str) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "cancel_ci_run".into() })
}

pub async fn list_ci_workflows(repo: &RepoRef) -> Result<Vec<CiWorkflow>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    crate::git_provider::ci_impl::list_github_workflows(owner, name, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn create_ci_pipeline(
    repo: &RepoRef,
    req:  PipelineCreateRequest,
) -> Result<CiRun, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let token = token()?;
    let workflow_id = req.workflow_id.clone().ok_or_else(|| {
        ProviderError::BadRequest("workflow_id required for GitHub workflow_dispatch".into())
    })?;
    crate::git_provider::ci_impl::create_github_dispatch(
        owner,
        name,
        &workflow_id,
        &req.branch,
        &req.variables,
        &token,
    )
    .await
    .map_err(ProviderError::from)?;
    // GitHub workflow_dispatch returns 204 No Content; the run shows up
    // asynchronously. Return a placeholder CiRun — callers refresh via
    // `list_ci_runs` after a short delay.
    Ok(CiRun {
        id:            String::new(),
        name:          req.workflow_id.unwrap_or_else(|| "dispatched".into()),
        status:        "pending".into(),
        branch:        req.branch.clone(),
        commit_sha:    String::new(),
        web_url:       String::new(),
        created_at:    String::new(),
        provider:      "github".into(),
        duration_secs: None,
    })
}
