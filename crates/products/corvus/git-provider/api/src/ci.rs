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

impl CiProviderInfo {
    /// Pure provider detection from a list of remotes — prefers `origin`, then
    /// the first GitHub/GitLab match. `None` when no remote is a GitHub/GitLab
    /// URL. The `has_token` field is left `false` for the caller to fill, since
    /// the token probe is keyring-coupled and out of this crate's scope (the
    /// shell reads it shell-side; the OOP backend fills it from the resolved
    /// provider's `has_token()`).
    pub fn detect_from_remotes(remotes: &[(String, String)]) -> Option<Self> {
        let ordered = remotes
            .iter()
            .filter(|(n, _)| n == "origin")
            .chain(remotes.iter().filter(|(n, _)| n != "origin"));
        for (_, url) in ordered {
            if let Some(info) = Self::detect_from_url(url) {
                return Some(info);
            }
        }
        None
    }

    /// Pure provider detection from a single remote URL (see
    /// [`detect_from_remotes`](Self::detect_from_remotes) for the `has_token`
    /// caveat).
    pub fn detect_from_url(url: &str) -> Option<Self> {
        if url.contains("github.com") {
            let (owner, repo) = parse_github_url(url)?;
            return Some(Self {
                provider:        "github".into(),
                remote_url:      url.to_string(),
                has_token:       false,
                owner:           Some(owner),
                repo_name:       Some(repo),
                project_path:    None,
                gitlab_base_url: None,
            });
        }
        // Accept gitlab.com and any self-hosted GitLab (gitlab.*).
        if url.contains("gitlab.com") || url.contains("gitlab.") {
            let (base_url, path) = parse_gitlab_url(url)?;
            return Some(Self {
                provider:        "gitlab".into(),
                remote_url:      url.to_string(),
                has_token:       false,
                owner:           None,
                repo_name:       None,
                project_path:    Some(path),
                gitlab_base_url: Some(base_url),
            });
        }
        None
    }
}

fn parse_github_url(url: &str) -> Option<(String, String)> {
    let path = if let Some(r) = url
        .strip_prefix("https://github.com/")
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
    let repo = parts.next()?.to_string();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner, repo))
}

fn parse_gitlab_url(url: &str) -> Option<(String, String)> {
    if let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) {
        let without_git = rest.trim_end_matches(".git");
        if let Some(slash) = without_git.find('/') {
            let base = &without_git[..slash];
            let path = &without_git[slash + 1..];
            if path.is_empty() {
                return None;
            }
            return Some((format!("https://{base}"), path.to_string()));
        }
    } else if let Some(rest) = url.strip_prefix("git@") {
        let without_git = rest.trim_end_matches(".git");
        if let Some(colon) = without_git.find(':') {
            let base = &without_git[..colon];
            let path = &without_git[colon + 1..];
            if path.is_empty() {
                return None;
            }
            return Some((format!("https://{base}"), path.to_string()));
        }
    }
    None
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
    /// MR/PR number — when set, returns runs scoped to that MR (GitHub merges
    /// branch + head-sha runs; GitLab merges MR-pipeline + branch runs).
    pub mr_number: Option<u64>,
    /// Head commit SHA of the MR/PR. GitHub additionally queries runs pinned to
    /// this SHA (fork PRs, `workflow_dispatch`) and merges them with the branch
    /// runs. Ignored by GitLab.
    pub head_sha: Option<String>,
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

#[cfg(test)]
mod tests {
    use super::CiProviderInfo;

    #[test]
    fn detects_github_https_and_ssh() {
        let https = CiProviderInfo::detect_from_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(https.provider, "github");
        assert_eq!(https.owner.as_deref(), Some("owner"));
        assert_eq!(https.repo_name.as_deref(), Some("repo"));
        let ssh = CiProviderInfo::detect_from_url("git@github.com:owner/repo.git").unwrap();
        assert_eq!(ssh.owner.as_deref(), Some("owner"));
        assert_eq!(ssh.repo_name.as_deref(), Some("repo"));
    }

    #[test]
    fn detects_gitlab_hosted_and_self_hosted() {
        let hosted = CiProviderInfo::detect_from_url("https://gitlab.com/group/sub/proj.git").unwrap();
        assert_eq!(hosted.provider, "gitlab");
        assert_eq!(hosted.gitlab_base_url.as_deref(), Some("https://gitlab.com"));
        assert_eq!(hosted.project_path.as_deref(), Some("group/sub/proj"));
        let self_hosted = CiProviderInfo::detect_from_url("https://gitlab.example.org/team/app").unwrap();
        assert_eq!(self_hosted.gitlab_base_url.as_deref(), Some("https://gitlab.example.org"));
        assert_eq!(self_hosted.project_path.as_deref(), Some("team/app"));
    }

    #[test]
    fn prefers_origin_then_first_match_and_skips_non_providers() {
        let remotes = vec![
            ("upstream".to_string(), "https://github.com/up/stream.git".to_string()),
            ("origin".to_string(), "https://gitlab.com/me/app.git".to_string()),
        ];
        let info = CiProviderInfo::detect_from_remotes(&remotes).unwrap();
        assert_eq!(info.provider, "gitlab");
        assert_eq!(info.project_path.as_deref(), Some("me/app"));

        let no_provider = vec![("origin".to_string(), "https://example.com/x/y.git".to_string())];
        assert!(CiProviderInfo::detect_from_remotes(&no_provider).is_none());
    }

    #[test]
    fn has_token_is_left_false_for_the_caller() {
        let info = CiProviderInfo::detect_from_url("https://github.com/o/r").unwrap();
        assert!(!info.has_token, "pure detect must not probe the keyring");
    }
}
