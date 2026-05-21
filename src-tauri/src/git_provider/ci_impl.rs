use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};
use chrono::{DateTime, Utc};

// OAuth refresh serialization lives next to the refresh implementation in
// `crate::git_provider::oauth::{gitlab_flow, github_flow}`. The
// `try_refresh_if_stale` helpers there acquire the lock and coalesce
// concurrent 401-driven refreshes for us — the senders below just call them.

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Information about a detected CI/CD provider for a repo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiProviderInfo {
    /// "github" | "gitlab"
    pub provider: String,
    pub remote_url: String,
    /// True when an OAuth token is available for this provider.
    pub has_token: bool,
    /// GitHub: repository owner (login).
    pub owner: Option<String>,
    /// GitHub: repository name.
    pub repo_name: Option<String>,
    /// GitLab: namespace + path, e.g. "myorg/myrepo".
    pub project_path: Option<String>,
    /// GitLab: API base URL (https://gitlab.com for hosted; custom for self-hosted).
    pub gitlab_base_url: Option<String>,
}

/// A single CI pipeline run / workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiRun {
    pub id: String,
    pub name: String,
    /// "pending" | "running" | "success" | "failed" | "cancelled"
    pub status: String,
    /// Branch / ref name.
    pub branch: String,
    /// Short (8-char) commit SHA.
    pub commit_sha: String,
    /// URL to open in the browser.
    pub web_url: String,
    /// ISO 8601 creation timestamp (let the frontend parse it).
    pub created_at: String,
    /// "github" | "gitlab"
    pub provider: String,
    /// Wall-clock duration in seconds (None when still running or unknown).
    pub duration_secs: Option<f64>,
}

/// A GitHub Actions workflow definition (used for the "create pipeline" modal).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWorkflow {
    pub id:   String,
    pub name: String,
    /// Relative path inside the repo, e.g. ".github/workflows/ci.yml".
    pub path: String,
}

/// A single job within a CI pipeline run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiJob {
    pub id: String,
    pub name: String,
    /// Stage name — "Jobs" for GitHub (no native stage concept).
    pub stage: String,
    /// Same status vocabulary as CiRun.
    pub status: String,
    /// Wall-clock duration in seconds.
    pub duration_secs: Option<f64>,
    /// URL to open in the browser for job logs.
    pub web_url: String,
    /// When true, pipeline success is not blocked by this job's failure.
    pub allow_failure: bool,
}

// ---------------------------------------------------------------------------
// Provider detection
// ---------------------------------------------------------------------------

/// Given a list of remote URLs, detect the first GitHub or GitLab remote.
/// Prefers "origin"; otherwise returns the first match.
pub fn detect_from_remotes(
    remotes: &[(String, String)], // (name, url)
) -> Option<CiProviderInfo> {
    // Prefer "origin", then take the first matching remote.
    let ordered = remotes.iter()
        .filter(|(n, _)| n == "origin")
        .chain(remotes.iter().filter(|(n, _)| n != "origin"));

    for (_, url) in ordered {
        if let Some(info) = detect_from_url(url) {
            return Some(info);
        }
    }
    None
}

/// Detect provider from a single remote URL.
pub fn detect_from_url(url: &str) -> Option<CiProviderInfo> {
    if url.contains("github.com") {
        let (owner, repo) = parse_github_url(url)?;
        let has_token = get_github_token().ok().flatten().is_some();
        return Some(CiProviderInfo {
            provider:         "github".into(),
            remote_url:       url.to_string(),
            has_token,
            owner:            Some(owner),
            repo_name:        Some(repo),
            project_path:     None,
            gitlab_base_url:  None,
        });
    }
    // Accept gitlab.com and any self-hosted GitLab (gitlab.*)
    if url.contains("gitlab.com") || url.contains("gitlab.") {
        let (base_url, path) = parse_gitlab_url(url)?;
        // For self-hosted GitLab we can't use the generic "gitlab.com/arbor" token;
        // fall back to host-based credential store.
        let has_token = if base_url.contains("gitlab.com") {
            get_gitlab_token(&base_url).ok().flatten().is_some()
        } else {
            crate::auth::credential_store::get_for_host(&base_url)
                .ok()
                .flatten()
                .is_some()
        };
        return Some(CiProviderInfo {
            provider:         "gitlab".into(),
            remote_url:       url.to_string(),
            has_token,
            owner:            None,
            repo_name:        None,
            project_path:     Some(path),
            gitlab_base_url:  Some(base_url),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// URL parsers
// ---------------------------------------------------------------------------

fn parse_github_url(url: &str) -> Option<(String, String)> {
    let path = if let Some(r) = url.strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        r
    } else if let Some(r) = url.strip_prefix("git@github.com:") {
        r
    } else {
        return None;
    };
    let path = path.trim_end_matches(".git");
    let mut parts = path.splitn(2, '/');
    let owner = parts.next()?.to_string();
    let repo  = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() { return None; }
    Some((owner, repo))
}

fn parse_gitlab_url(url: &str) -> Option<(String, String)> {
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let without_git = rest.trim_end_matches(".git");
        if let Some(slash) = without_git.find('/') {
            let base = &without_git[..slash];
            let path = &without_git[slash + 1..];
            if path.is_empty() { return None; }
            return Some((format!("https://{base}"), path.to_string()));
        }
    } else if let Some(rest) = url.strip_prefix("git@") {
        let without_git = rest.trim_end_matches(".git");
        if let Some(colon) = without_git.find(':') {
            let base = &without_git[..colon];
            let path = &without_git[colon + 1..];
            if path.is_empty() { return None; }
            return Some((format!("https://{base}"), path.to_string()));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Token retrieval
// ---------------------------------------------------------------------------

pub fn get_github_token() -> Result<Option<String>> {
    let oauth = crate::auth::credential_store::get("github.com/arbor", "oauth")?;
    if oauth.is_some() {
        return Ok(oauth);
    }
    Ok(crate::auth::credential_store::get_for_host("github.com")?
        .map(|(_, tok)| tok))
}

/// Returns the token for GitLab — either the stored OAuth token (gitlab.com)
/// or a host-based credential (self-hosted instances).
pub fn get_gitlab_token(base_url: &str) -> Result<Option<String>> {
    if base_url.contains("gitlab.com") {
        let oauth = crate::auth::credential_store::get("gitlab.com/arbor", "oauth")?;
        if oauth.is_some() {
            return Ok(oauth);
        }
        Ok(crate::auth::credential_store::get_for_host("gitlab.com")?
            .map(|(_, tok)| tok))
    } else {
        Ok(crate::auth::credential_store::get_for_host(base_url)?
            .map(|(_, tok)| tok))
    }
}

// ---------------------------------------------------------------------------
// GitLab request helper — automatic token refresh on 401
// ---------------------------------------------------------------------------

/// Send a GitLab API request built by `make_req(token) → RequestBuilder`.
///
/// On HTTP 401 the stored OAuth refresh token is used to obtain a new access
/// token (gitlab.com Device Flow only); the request is then retried once.
/// Self-hosted instances that use PAT credentials are not refreshed.
pub(crate) async fn gitlab_send_with_refresh<F>(
    make_req:      F,
    base_url:      &str,
    current_token: &str,
) -> Result<reqwest::Response>
where
    F: Fn(&str) -> reqwest::RequestBuilder,
{
    let resp = make_req(current_token)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GitLab API request failed: {e}")))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED && base_url.contains("gitlab.com") {
        // The serialization + coalescence happens inside try_refresh_if_stale:
        // it takes the GitLab refresh lock and, if another task already
        // rotated the token while we were queued, returns Ok(true) without
        // calling /oauth/token again (which would fail since GitLab rotates
        // refresh tokens single-use).
        let refreshed = crate::git_provider::oauth::gitlab_flow::try_refresh_if_stale(Some(current_token))
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("GitLab token refresh error: {e}");
                false
            });
        if refreshed {
            if let Ok(Some(new_token)) = get_gitlab_token(base_url) {
                return make_req(&new_token)
                    .send()
                    .await
                    .map_err(|e| AppError::Other(format!("GitLab API request failed: {e}")));
            }
        }
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!(
            "GitLab API 401 Unauthorized: {body}"
        )));
    }

    Ok(resp)
}

// ---------------------------------------------------------------------------
// GitHub request helper — automatic token refresh on 401
// ---------------------------------------------------------------------------

/// Send a GitHub API request built by `make_req(token) → RequestBuilder`.
///
/// On HTTP 401 the stored OAuth refresh token is used to obtain a new access
/// token (only available when the OAuth App has token-expiration enabled).
/// The request is then retried once.
pub(crate) async fn github_send_with_refresh<F>(
    make_req:      F,
    current_token: &str,
) -> Result<reqwest::Response>
where
    F: Fn(&str) -> reqwest::RequestBuilder,
{
    let resp = make_req(current_token)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GitHub API request failed: {e}")))?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        // Same coalescing guard as the GitLab helper — implemented inside
        // try_refresh_if_stale.
        let refreshed = crate::git_provider::oauth::github_flow::try_refresh_if_stale(Some(current_token))
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("GitHub token refresh error: {e}");
                false
            });
        if refreshed {
            if let Ok(Some(new_token)) = get_github_token() {
                return make_req(&new_token)
                    .send()
                    .await
                    .map_err(|e| AppError::Other(format!("GitHub API request failed: {e}")));
            }
        }
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!(
            "GitHub API 401 Unauthorized: {body}"
        )));
    }

    Ok(resp)
}

// ---------------------------------------------------------------------------
// GitHub Actions API
// ---------------------------------------------------------------------------

pub async fn fetch_github_runs(
    owner: &str,
    repo:  &str,
    token: &str,
) -> Result<Vec<CiRun>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs?per_page=30"
    );
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub API {status}: {body}")));
    }

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

    let parsed: RunsResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitHub API parse error: {e}")))?;

    Ok(parsed.workflow_runs.into_iter().map(|r| {
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
    }).collect())
}

/// Same as `fetch_github_runs` but filtered server-side to runs whose
/// `head_branch` matches `branch`. Used by the MR/PR detail modal.
pub async fn fetch_github_runs_for_branch(
    owner:  &str,
    repo:   &str,
    branch: &str,
    token:  &str,
) -> Result<Vec<CiRun>> {
    fetch_github_runs_with_query(owner, repo, &[("branch", branch)], token).await
}

/// Variant filtered server-side by `head_sha`. Useful as a complement to the
/// branch query for PRs from forks and workflows that don't tag the source
/// branch on the run (e.g. `pull_request_target`).
pub async fn fetch_github_runs_for_sha(
    owner:  &str,
    repo:   &str,
    sha:    &str,
    token:  &str,
) -> Result<Vec<CiRun>> {
    fetch_github_runs_with_query(owner, repo, &[("head_sha", sha)], token).await
}

/// Internal: shared GET on `/actions/runs` with arbitrary query filters.
async fn fetch_github_runs_with_query(
    owner:  &str,
    repo:   &str,
    extra:  &[(&str, &str)],
    token:  &str,
) -> Result<Vec<CiRun>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs?per_page=30"
    );
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.get(&url)
            .query(extra)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub API {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct RunsResponse { workflow_runs: Vec<GhRun> }
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

    let parsed: RunsResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitHub API parse error: {e}")))?;

    Ok(parsed.workflow_runs.into_iter().map(|r| {
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
    }).collect())
}

pub async fn retrigger_github_run(
    owner:  &str,
    repo:   &str,
    run_id: &str,
    token:  &str,
) -> Result<()> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs/{run_id}/rerun"
    );
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0")
            .header("Content-Length", "0"),
        token,
    ).await?;

    // 201 Created is the success response for this endpoint.
    if resp.status().is_success() || resp.status().as_u16() == 201 {
        return Ok(());
    }
    let status = resp.status();
    let body   = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub retrigger {status}: {body}")))
}

// ---------------------------------------------------------------------------
// GitLab CI API
// ---------------------------------------------------------------------------

pub async fn fetch_gitlab_pipelines(
    project_path: &str,
    base_url:     &str,
    token:        &str,
) -> Result<Vec<CiRun>> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/pipelines?per_page=30&order_by=id&sort=desc"
    );
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab API {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct GlPipeline {
        id:         i64,
        status:     String,
        #[serde(rename = "ref")]
        branch:     String,
        sha:        String,
        web_url:    String,
        created_at: String,
        /// Wall-clock duration in seconds (nullable in API).
        duration:   Option<f64>,
    }

    let pipelines: Vec<GlPipeline> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitLab API parse error: {e}")))?;

    Ok(pipelines.into_iter().map(|p| {
        let sha = &p.sha[..8.min(p.sha.len())];
        CiRun {
            id:            p.id.to_string(),
            name:          format!("Pipeline #{}", p.id),
            status:        map_gitlab_status(&p.status),
            branch:        p.branch,
            commit_sha:    sha.to_string(),
            web_url:       p.web_url,
            created_at:    p.created_at,
            provider:      "gitlab".into(),
            duration_secs: p.duration,
        }
    }).collect())
}

/// Fetch pipelines tied to a specific GitLab Merge Request.
///
/// Uses `GET /projects/:id/merge_requests/:iid/pipelines`, which returns
/// pipelines associated with the MR — including **detached merge-request
/// pipelines** whose `ref` is `refs/merge-requests/{iid}/head` and therefore
/// don't show up when filtering the regular pipelines list by source branch.
pub async fn fetch_gitlab_mr_pipelines(
    project_path: &str,
    base_url:     &str,
    mr_iid:       i64,
    token:        &str,
) -> Result<Vec<CiRun>> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/merge_requests/{mr_iid}/pipelines"
    );
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab MR pipelines API {status}: {body}")));
    }

    // The MR pipelines endpoint is a slim variant — no `duration` field.
    #[derive(Deserialize)]
    struct GlMrPipeline {
        id:         i64,
        status:     String,
        #[serde(rename = "ref")]
        branch:     String,
        sha:        String,
        web_url:    String,
        created_at: String,
        updated_at: Option<String>,
    }

    let pipelines: Vec<GlMrPipeline> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitLab MR pipelines parse error: {e}")))?;

    Ok(pipelines.into_iter().map(|p| {
        let sha = &p.sha[..8.min(p.sha.len())];
        let dur = parse_iso_duration(Some(&p.created_at), p.updated_at.as_deref());
        CiRun {
            id:            p.id.to_string(),
            name:          format!("Pipeline #{}", p.id),
            status:        map_gitlab_status(&p.status),
            branch:        p.branch,
            commit_sha:    sha.to_string(),
            web_url:       p.web_url,
            created_at:    p.created_at,
            provider:      "gitlab".into(),
            duration_secs: dur,
        }
    }).collect())
}

pub async fn retrigger_gitlab_pipeline(
    project_path: &str,
    base_url:     &str,
    pipeline_id:  &str,
    token:        &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/pipelines/{pipeline_id}/retry"
    );
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .header("Content-Length", "0"),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body   = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab retrigger {status}: {body}")))
}

// ---------------------------------------------------------------------------
// GitHub: fetch jobs for a single workflow run
// ---------------------------------------------------------------------------

pub async fn fetch_github_jobs(
    owner:  &str,
    repo:   &str,
    run_id: &str,
    token:  &str,
) -> Result<Vec<CiJob>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/actions/runs/{run_id}/jobs?per_page=100"
    );
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub jobs API {status}: {body}")));
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

    let parsed: JobsResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitHub jobs parse error: {e}")))?;

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

// ---------------------------------------------------------------------------
// GitLab: fetch jobs for a single pipeline
// ---------------------------------------------------------------------------

pub async fn fetch_gitlab_jobs(
    project_path: &str,
    base_url:     &str,
    pipeline_id:  &str,
    token:        &str,
) -> Result<Vec<CiJob>> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/pipelines/{pipeline_id}/jobs?per_page=100"
    );
    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab jobs API {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct GlJob {
        id:            i64,
        name:          String,
        stage:         String,
        status:        String,
        duration:      Option<f64>,
        web_url:       String,
        allow_failure: bool,
    }

    let jobs: Vec<GlJob> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitLab jobs parse error: {e}")))?;

    Ok(jobs.into_iter().map(|j| CiJob {
        id:            j.id.to_string(),
        name:          j.name,
        stage:         j.stage,
        status:        map_gitlab_status(&j.status),
        duration_secs: j.duration,
        web_url:       j.web_url,
        allow_failure: j.allow_failure,
    }).collect())
}

// ---------------------------------------------------------------------------
// GitHub: list workflows (for the "create run" modal)
// ---------------------------------------------------------------------------

pub async fn list_github_workflows(
    owner: &str,
    repo:  &str,
    token: &str,
) -> Result<Vec<CiWorkflow>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/actions/workflows");
    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0"),
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub workflows API {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct WfResponse { workflows: Vec<GhWorkflow> }
    #[derive(Deserialize)]
    struct GhWorkflow { id: i64, name: String, path: String, state: String }

    let parsed: WfResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitHub workflows parse error: {e}")))?;

    Ok(parsed.workflows.into_iter()
        .filter(|w| w.state == "active")
        .map(|w| CiWorkflow { id: w.id.to_string(), name: w.name, path: w.path })
        .collect())
}

// ---------------------------------------------------------------------------
// GitHub: trigger a workflow_dispatch event
// ---------------------------------------------------------------------------

pub async fn create_github_dispatch(
    owner:       &str,
    repo:        &str,
    workflow_id: &str,
    branch:      &str,
    inputs:      &[(String, String)],
    token:       &str,
) -> Result<()> {
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

    let client = reqwest::Client::new();
    let resp = github_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        token,
    ).await?;

    // 204 No Content = success; GitHub does not return a run ID immediately.
    if resp.status().as_u16() == 204 || resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body   = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub dispatch {status}: {body}")))
}

// ---------------------------------------------------------------------------
// GitLab: create a new pipeline run
// ---------------------------------------------------------------------------

pub async fn create_gitlab_pipeline(
    project_path: &str,
    base_url:     &str,
    branch:       &str,
    variables:    &[(String, String)],
    token:        &str,
) -> Result<String> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{base_url}/api/v4/projects/{encoded}/pipeline");

    #[derive(Serialize)]
    struct Body { r#ref: String, variables: Vec<Var> }
    #[derive(Serialize)]
    struct Var { key: String, value: String, variable_type: &'static str }

    let body = Body {
        r#ref: branch.to_string(),
        variables: variables.iter().map(|(k, v)| Var {
            key:           k.clone(),
            value:         v.clone(),
            variable_type: "env_var",
        }).collect(),
    };

    let client = reqwest::Client::new();
    let resp = gitlab_send_with_refresh(
        |tok| client.post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab create pipeline {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct Created { id: i64 }

    let created: Created = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("GitLab create pipeline parse error: {e}")))?;

    Ok(created.id.to_string())
}

// ---------------------------------------------------------------------------
// Helpers
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

fn map_gitlab_status(s: &str) -> String {
    match s {
        "success" | "passed"                                                => "success",
        "failed"                                                            => "failed",
        "canceled" | "skipped"                                              => "cancelled",
        "running"                                                           => "running",
        "pending" | "created" | "waiting_for_resource"
        | "preparing" | "scheduled"                                         => "pending",
        _                                                                   => "pending",
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

/// Percent-encode `/` as `%2F` for GitLab project path in URL segments.
fn percent_encode_slash(s: &str) -> String {
    s.replace('/', "%2F")
}
