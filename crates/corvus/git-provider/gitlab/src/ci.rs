//! GitLab CI/CD operations — keyring-free port.
//!
//! Folds the old `git_provider::gitlab::ci` delegate (RepoRef destructuring,
//! placeholder/return shaping, Unsupported features) together with the GitLab
//! REST bodies from `git_provider::ci_impl::*`, all routed through `GitlabHttp`.

use serde::{Deserialize, Serialize};

use corvus_git_provider_api::prelude::*;

use crate::http::{classify, percent_encode_slash, GitlabHttp};

// ---------------------------------------------------------------------------
// Delegate helpers (RepoRef destructuring)
// ---------------------------------------------------------------------------

fn project_path<'a>(repo: &'a RepoRef) -> &'a str {
    repo.owner_or_path.as_str()
}

// ---------------------------------------------------------------------------
// Trait surface (mirrors the delegate's public fns)
// ---------------------------------------------------------------------------

pub(crate) async fn list_ci_runs(
    http: &GitlabHttp,
    repo: &RepoRef,
    _filter: CiFilter,
) -> Result<Vec<CiRun>, ProviderError> {
    let path = project_path(repo);
    fetch_gitlab_pipelines(http, path).await
}

pub(crate) async fn get_ci_run(
    _http: &GitlabHttp,
    _repo: &RepoRef,
    _run_id: &str,
) -> Result<CiRun, ProviderError> {
    Err(ProviderError::Unsupported { feature: "get_ci_run".into() })
}

pub(crate) async fn fetch_ci_jobs(
    http: &GitlabHttp,
    repo: &RepoRef,
    run_id: &str,
) -> Result<Vec<CiJob>, ProviderError> {
    let path = project_path(repo);
    fetch_gitlab_jobs(http, path, run_id).await
}

pub(crate) async fn fetch_ci_job_log(
    _http: &GitlabHttp,
    _repo: &RepoRef,
    _job_id: &str,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_ci_job_log".into() })
}

pub(crate) async fn retrigger_ci_run(
    http: &GitlabHttp,
    repo: &RepoRef,
    run_id: &str,
) -> Result<(), ProviderError> {
    let path = project_path(repo);
    retrigger_gitlab_pipeline(http, path, run_id).await
}

pub(crate) async fn cancel_ci_run(
    _http: &GitlabHttp,
    _repo: &RepoRef,
    _run_id: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "cancel_ci_run".into() })
}

pub(crate) async fn list_ci_workflows(
    _http: &GitlabHttp,
    _repo: &RepoRef,
) -> Result<Vec<CiWorkflow>, ProviderError> {
    // GitLab pipelines aren't named workflows the way GitHub Actions are —
    // the editor lives at .gitlab-ci.yml. The frontend already hides the
    // workflow picker for GitLab; surface it here as Unsupported so the
    // capability is honest.
    Err(ProviderError::Unsupported { feature: "list_ci_workflows".into() })
}

pub(crate) async fn create_ci_pipeline(
    http: &GitlabHttp,
    repo: &RepoRef,
    req: PipelineCreateRequest,
) -> Result<CiRun, ProviderError> {
    let path = project_path(repo);
    let id = create_gitlab_pipeline(http, path, &req.branch, &req.variables).await?;
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

// ---------------------------------------------------------------------------
// GitLab CI REST (ported from ci_impl, behavior-preserving)
// ---------------------------------------------------------------------------

async fn fetch_gitlab_pipelines(
    http: &GitlabHttp,
    project_path: &str,
) -> Result<Vec<CiRun>, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/pipelines?per_page=30&order_by=id&sort=desc",
        http.base()
    );
    let resp = http.send(|s| http.client().get(&url)
        .header("Authorization", &s.auth_header)
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab API {status}: {body}")));
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
        .map_err(|e| classify(format!("GitLab API parse error: {e}")))?;

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

async fn retrigger_gitlab_pipeline(
    http: &GitlabHttp,
    project_path: &str,
    pipeline_id:  &str,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/pipelines/{pipeline_id}/retry",
        http.base()
    );
    let resp = http.send(|s| http.client().post(&url)
        .header("Authorization", &s.auth_header)
        .header("User-Agent", "arbor-git-gui/1.0")
        .header("Content-Length", "0")).await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body   = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab retrigger {status}: {body}")))
}

async fn fetch_gitlab_jobs(
    http: &GitlabHttp,
    project_path: &str,
    pipeline_id:  &str,
) -> Result<Vec<CiJob>, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/pipelines/{pipeline_id}/jobs?per_page=100",
        http.base()
    );
    let resp = http.send(|s| http.client().get(&url)
        .header("Authorization", &s.auth_header)
        .header("User-Agent", "arbor-git-gui/1.0")).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab jobs API {status}: {body}")));
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
        .map_err(|e| classify(format!("GitLab jobs parse error: {e}")))?;

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

async fn create_gitlab_pipeline(
    http: &GitlabHttp,
    project_path: &str,
    branch:       &str,
    variables:    &[(String, String)],
) -> Result<String, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{}/api/v4/projects/{encoded}/pipeline", http.base());

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

    let resp = http.send(|s| http.client().post(&url)
        .header("Authorization", &s.auth_header)
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)).await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab create pipeline {status}: {body}")));
    }

    #[derive(Deserialize)]
    struct Created { id: i64 }

    let created: Created = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab create pipeline parse error: {e}")))?;

    Ok(created.id.to_string())
}

// ---------------------------------------------------------------------------
// Helpers (GitLab-only; map_github_status/parse_iso_duration are GitHub, not ported)
// ---------------------------------------------------------------------------

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
