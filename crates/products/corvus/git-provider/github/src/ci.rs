//! GitHub Actions (CI) operations — keyring-free port.
//!
//! Folds the old `git_provider::github::ci` delegate (RepoRef destructuring,
//! placeholder/return shaping, Unsupported features) together with the GitHub
//! REST bodies from `git_provider::ci_impl::*`, all routed through `GithubHttp`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use corvus_git_provider_api::prelude::*;

use crate::http::{classify, GithubHttp};

// ---------------------------------------------------------------------------
// Delegate helpers (RepoRef destructuring / validation)
// ---------------------------------------------------------------------------

fn repo_parts(repo: &RepoRef) -> Result<(&str, &str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo.name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub RepoRef requires name".into())
    })?;
    Ok((owner, name))
}

// ---------------------------------------------------------------------------
// Trait surface (mirrors the delegate's public fns)
// ---------------------------------------------------------------------------

pub(crate) async fn list_ci_runs(
    http: &GithubHttp,
    repo: &RepoRef,
    filter: CiFilter,
) -> Result<Vec<CiRun>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    // MR/PR-scoped aggregation: both providers can attach runs to a PR via
    // paths a plain branch filter misses (fork PRs, `pull_request_target`,
    // manual dispatch pinned to the head SHA). We hit `?branch=` and
    // `?head_sha=` concurrently and dedupe by run id. The branch query is
    // primary (its error surfaces); the head-sha query is best-effort.
    if filter.mr_number.is_some() {
        let branch = filter.branch.as_deref().unwrap_or("");
        let by_branch_fut = fetch_github_runs_for_branch(http, owner, name, branch);
        let by_sha_fut = async {
            if let Some(sha) = filter.head_sha.as_deref().filter(|s| !s.is_empty()) {
                fetch_github_runs_for_sha(http, owner, name, sha).await
            } else {
                Ok(Vec::new())
            }
        };
        let (branch_res, sha_res) = tokio::join!(by_branch_fut, by_sha_fut);

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
        return Ok(runs);
    }
    if let Some(branch) = filter.branch.as_deref() {
        fetch_github_runs_for_branch(http, owner, name, branch).await
    } else {
        fetch_github_runs(http, owner, name).await
    }
}

pub(crate) async fn get_ci_run(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _run_id: &str,
) -> Result<CiRun, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_ci_run".into() })
}

pub(crate) async fn fetch_ci_jobs(
    http: &GithubHttp,
    repo: &RepoRef,
    run_id: &str,
) -> Result<Vec<CiJob>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    fetch_github_jobs(http, owner, name, run_id).await
}

pub(crate) async fn fetch_ci_job_log(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _job_id: &str,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_ci_job_log".into() })
}

pub(crate) async fn retrigger_ci_run(
    http: &GithubHttp,
    repo: &RepoRef,
    run_id: &str,
) -> Result<(), ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    retrigger_github_run(http, owner, name, run_id).await
}

pub(crate) async fn cancel_ci_run(
    _http: &GithubHttp,
    _repo: &RepoRef,
    _run_id: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "cancel_ci_run".into() })
}

pub(crate) async fn list_ci_workflows(
    http: &GithubHttp,
    repo: &RepoRef,
) -> Result<Vec<CiWorkflow>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    list_github_workflows(http, owner, name).await
}

pub(crate) async fn create_ci_pipeline(
    http: &GithubHttp,
    repo: &RepoRef,
    req: PipelineCreateRequest,
) -> Result<CiRun, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let workflow_id = req.workflow_id.clone().ok_or_else(|| {
        ProviderError::BadRequest("workflow_id required for GitHub workflow_dispatch".into())
    })?;
    create_github_dispatch(
        http,
        owner,
        name,
        &workflow_id,
        &req.branch,
        &req.variables,
    )
    .await?;
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

// ---------------------------------------------------------------------------
// GitHub Actions REST (ported from ci_impl, behavior-preserving)
// ---------------------------------------------------------------------------

async fn fetch_github_runs(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
) -> Result<Vec<CiRun>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs?per_page=30"
    );
    let resp = http.send(|s| http.client().get(&url)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub API {status}: {body}")));
    }

    let parsed: RunsResponse = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub API parse error: {e}")))?;

    Ok(parsed.workflow_runs.into_iter().map(map_run).collect())
}

/// Same as `fetch_github_runs` but filtered server-side to runs whose
/// `head_branch` matches `branch`. Used by the MR/PR detail modal.
async fn fetch_github_runs_for_branch(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Vec<CiRun>, ProviderError> {
    fetch_github_runs_with_query(http, owner, repo, &[("branch", branch)]).await
}

/// Variant filtered server-side by `head_sha`. Useful as a complement to the
/// branch query for PRs from forks and workflows that don't tag the source
/// branch on the run (e.g. `pull_request_target`).
async fn fetch_github_runs_for_sha(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Vec<CiRun>, ProviderError> {
    fetch_github_runs_with_query(http, owner, repo, &[("head_sha", sha)]).await
}

/// Internal: shared GET on `/actions/runs` with arbitrary query filters.
async fn fetch_github_runs_with_query(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    extra: &[(&str, &str)],
) -> Result<Vec<CiRun>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs?per_page=30"
    );
    let resp = http.send(|s| http.client().get(&url)
        .query(extra)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub API {status}: {body}")));
    }

    let parsed: RunsResponse = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub API parse error: {e}")))?;

    Ok(parsed.workflow_runs.into_iter().map(map_run).collect())
}

async fn retrigger_github_run(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    run_id: &str,
) -> Result<(), ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs/{run_id}/rerun"
    );
    let resp = http.send(|s| http.client().post(&url)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .header("Content-Length", "0")).await?;

    // 201 Created is the success response for this endpoint.
    if resp.status().is_success() || resp.status().as_u16() == 201 {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub retrigger {status}: {body}")))
}

async fn fetch_github_jobs(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    run_id: &str,
) -> Result<Vec<CiJob>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs/{run_id}/jobs?per_page=100"
    );
    let resp = http.send(|s| http.client().get(&url)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub jobs API {status}: {body}")));
    }

    let parsed: JobsResponse = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub jobs parse error: {e}")))?;

    Ok(parsed.jobs.into_iter().map(|j| CiJob {
        id:            j.id.to_string(),
        name:          j.name,
        stage:         "Jobs".into(),
        status:        map_github_status(j.status.as_deref(), j.conclusion.as_deref()),
        duration_secs: parse_iso_duration(j.started_at.as_deref(), j.completed_at.as_deref()),
        web_url:       j.html_url,
        allow_failure: false,
    }).collect())
}

async fn list_github_workflows(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
) -> Result<Vec<CiWorkflow>, ProviderError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/actions/workflows");
    let resp = http.send(|s| http.client().get(&url)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub workflows API {status}: {body}")));
    }

    let parsed: WfResponse = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub workflows parse error: {e}")))?;

    Ok(parsed.workflows.into_iter()
        .filter(|w| w.state == "active")
        .map(|w| CiWorkflow { id: w.id.to_string(), name: w.name, path: w.path })
        .collect())
}

async fn create_github_dispatch(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    workflow_id: &str,
    branch: &str,
    inputs: &[(String, String)],
) -> Result<(), ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches"
    );

    // Build body with serde structs to avoid an extra dep.
    #[derive(Serialize)]
    struct Body {
        r#ref: String,
        inputs: std::collections::HashMap<String, String>,
    }
    let body = Body {
        r#ref: branch.to_string(),
        inputs: inputs.iter().cloned().collect(),
    };

    let resp = http.send(|s| http.client().post(&url)
        .header("Authorization", &s.auth_header)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)).await?;

    // 204 No Content = success; GitHub does not return a run ID immediately.
    if resp.status().as_u16() == 204 || resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub dispatch {status}: {body}")))
}

// ---------------------------------------------------------------------------
// Private response structs + mappers (copied verbatim from ci_impl)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct RunsResponse {
    workflow_runs: Vec<GhRun>,
}

#[derive(Deserialize)]
struct GhRun {
    id:              i64,
    name:            Option<String>,
    run_number:      i64,
    status:          Option<String>,
    conclusion:      Option<String>,
    head_branch:     Option<String>,
    head_sha:        String,
    html_url:        String,
    created_at:      String,
    run_started_at:  Option<String>,
    updated_at:      Option<String>,
}

fn map_run(r: GhRun) -> CiRun {
    let sha    = &r.head_sha[..8.min(r.head_sha.len())];
    let status = map_github_status(r.status.as_deref(), r.conclusion.as_deref());
    let dur    = if status != "running" && status != "pending" {
        parse_iso_duration(r.run_started_at.as_deref(), r.updated_at.as_deref())
    } else {
        None
    };
    CiRun {
        id:            r.id.to_string(),
        name:          r.name.unwrap_or_else(|| format!("Run #{}", r.run_number)),
        status,
        branch:        r.head_branch.unwrap_or_default(),
        commit_sha:    sha.to_string(),
        web_url:       r.html_url,
        created_at:    r.created_at,
        provider:      "github".into(),
        duration_secs: dur,
    }
}

#[derive(Deserialize)]
struct JobsResponse { jobs: Vec<GhJob> }

#[derive(Deserialize)]
struct GhJob {
    id:           i64,
    name:         String,
    status:       Option<String>,
    conclusion:   Option<String>,
    started_at:   Option<String>,
    completed_at: Option<String>,
    html_url:     String,
}

#[derive(Deserialize)]
struct WfResponse { workflows: Vec<GhWorkflow> }

#[derive(Deserialize)]
struct GhWorkflow { id: i64, name: String, path: String, state: String }

// ---------------------------------------------------------------------------
// Helpers (GitHub-relevant only; map_gitlab_status is GitLab-only, not ported)
// ---------------------------------------------------------------------------

fn map_github_status(status: Option<&str>, conclusion: Option<&str>) -> String {
    match status {
        Some("completed") => match conclusion {
            Some("success")                        => "success",
            Some("failure") | Some("timed_out")    => "failed",
            Some("cancelled") | Some("skipped")    => "cancelled",
            _                                      => "failed",
        },
        Some("in_progress") | Some("waiting")
        | Some("requested") | Some("queued")       => "running",
        _                                          => "pending",
    }
    .into()
}

/// Compute duration in seconds between two ISO 8601 timestamps.
fn parse_iso_duration(start: Option<&str>, end: Option<&str>) -> Option<f64> {
    let t1 = start?.parse::<DateTime<Utc>>().ok()?;
    let t2 = end?.parse::<DateTime<Utc>>().ok()?;
    let ms = (t2 - t1).num_milliseconds();
    if ms > 0 { Some(ms as f64 / 1000.0) } else { None }
}
