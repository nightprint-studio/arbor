use serde::{Deserialize, Serialize};

// ── CI/CD payloads ───────────────────────────────────────────────────────

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

// ── Trait-vocabulary request / filter types ──────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CiFilter {
    pub branch:  Option<String>,
    /// "running" | "success" | "failed" | "cancelled" | "pending"
    pub status:  Option<String>,
    /// MR/PR number — when set, returns runs scoped to that MR's source branch.
    pub mr_number: Option<u64>,
    pub page:     Option<u32>,
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCreateRequest {
    pub branch:    String,
    pub variables: Vec<(String, String)>,
    /// GitHub: workflow id or filename. None → first `workflow_dispatch` workflow.
    pub workflow_id: Option<String>,
}
