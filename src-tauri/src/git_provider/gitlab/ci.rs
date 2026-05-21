//! GitLab CI/CD operations.
//!
//! Phase 3 transition: every function delegates to the original
//! `crate::git_provider::ci_impl::*` implementation.

use crate::git_provider::types::{
    CiRun, CiJob, CiWorkflow, CiFilter, PipelineCreateRequest, RepoRef,
    error::ProviderError,
};

use super::api;

fn token(base_url: &str) -> Result<String, ProviderError> {
    api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(e.to_string()))?
        .ok_or(ProviderError::Unauthenticated)
}

fn project_path<'a>(repo: &'a RepoRef) -> &'a str {
    repo.owner_or_path.as_str()
}

pub async fn list_ci_runs(
    base_url: &str,
    repo:     &RepoRef,
    _filter:  CiFilter,
) -> Result<Vec<CiRun>, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    crate::git_provider::ci_impl::fetch_gitlab_pipelines(path, base_url, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn get_ci_run(
    _base_url: &str,
    _repo:     &RepoRef,
    _run_id:   &str,
) -> Result<CiRun, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_ci_run".into() })
}

pub async fn fetch_ci_jobs(
    base_url: &str,
    repo:     &RepoRef,
    run_id:   &str,
) -> Result<Vec<CiJob>, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    crate::git_provider::ci_impl::fetch_gitlab_jobs(path, base_url, run_id, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn fetch_ci_job_log(
    _base_url: &str,
    _repo:     &RepoRef,
    _job_id:   &str,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_ci_job_log".into() })
}

pub async fn retrigger_ci_run(
    base_url: &str,
    repo:     &RepoRef,
    run_id:   &str,
) -> Result<(), ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    crate::git_provider::ci_impl::retrigger_gitlab_pipeline(path, base_url, run_id, &token)
        .await
        .map_err(ProviderError::from)
}

pub async fn cancel_ci_run(
    _base_url: &str,
    _repo:     &RepoRef,
    _run_id:   &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "cancel_ci_run".into() })
}

pub async fn list_ci_workflows(
    _base_url: &str,
    _repo:     &RepoRef,
) -> Result<Vec<CiWorkflow>, ProviderError> {
    // GitLab pipelines aren't named workflows the way GitHub Actions are —
    // the editor lives at .gitlab-ci.yml. The frontend already hides the
    // workflow picker for GitLab; surface it here as Unsupported so the
    // capability is honest.
    Err(ProviderError::Unsupported { feature: "list_ci_workflows".into() })
}

pub async fn create_ci_pipeline(
    base_url: &str,
    repo:     &RepoRef,
    req:      PipelineCreateRequest,
) -> Result<CiRun, ProviderError> {
    let path  = project_path(repo);
    let token = token(base_url)?;
    let id = crate::git_provider::ci_impl::create_gitlab_pipeline(
        path,
        base_url,
        &req.branch,
        &req.variables,
        &token,
    )
    .await
    .map_err(ProviderError::from)?;
    // GitLab returns the new pipeline id; the full record arrives via
    // `list_ci_runs` on next refresh. Return a placeholder consistent with
    // the GitHub provider.
    Ok(CiRun {
        id,
        name:          format!("Pipeline on {}", req.branch),
        status:        "pending".into(),
        branch:        req.branch.clone(),
        commit_sha:    String::new(),
        web_url:       String::new(),
        created_at:    String::new(),
        provider:      "gitlab".into(),
        duration_secs: None,
    })
}
