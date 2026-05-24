use serde::{Deserialize, Serialize};
use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::{detect_from_remotes, CiProviderInfo};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MrState {
    Open,
    Closed,
    Merged,
}

impl std::fmt::Display for MrState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MrState::Open   => write!(f, "open"),
            MrState::Closed => write!(f, "closed"),
            MrState::Merged => write!(f, "merged"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrUser {
    pub login:        String,
    pub display_name: String,
    pub avatar_url:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrLabel {
    pub name:  String,
    pub color: String, // hex, e.g. "d73a4a"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrCheck {
    pub name:       String,
    /// "pending" | "running" | "success" | "failed" | "cancelled" | "skipped"
    pub status:     String,
    pub url:        Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrComment {
    pub id:         String,
    pub author:     MrUser,
    pub body:       String,
    pub created_at: String,
    /// Heuristic flag: true when the author looks like a bot account.
    /// GitHub: login ends with "[bot]" (the canonical bot suffix).
    /// GitLab: login or display name contains "bot" (case-insensitive).
    /// Lets the frontend hide automated comments by default.
    #[serde(default)]
    pub is_bot:     bool,
}

/// Activity entry for the MR/PR timeline — anything that's not a regular
/// user comment: state changes, label edits, assignments, force-pushes,
/// system notes, etc. Surfaced separately from `MrComment` so the UI can
/// filter Comments / Bots / Activity independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrEvent {
    pub id:         String,
    /// Coarse category — drives the icon and filter group on the frontend.
    /// Known values: "state" (closed/reopened/merged/draft toggles),
    /// "label", "assign", "review", "commit" (push/force-push),
    /// "rename", "system" (catch-all).
    pub kind:       String,
    /// The user who triggered the event. May be a bot for automated events.
    pub actor:      MrUser,
    /// Pre-rendered, human-readable summary ("added label bug",
    /// "force-pushed the source branch", "marked as ready for review", …).
    pub summary:    String,
    pub created_at: String,
}

/// Full information about a single Pull Request / Merge Request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeRequest {
    /// Provider-native numeric ID (PR number on GitHub, MR iid on GitLab).
    pub number:        u64,
    pub title:         String,
    pub description:   String,
    pub state:         MrState,
    pub is_draft:      bool,
    pub author:        MrUser,
    pub source_branch: String,
    pub target_branch: String,
    pub web_url:       String,
    pub created_at:    String,
    pub updated_at:    String,
    pub labels:        Vec<MrLabel>,
    pub assignees:     Vec<MrUser>,
    pub reviewers:     Vec<MrUser>,
    /// "pending" | "success" | "failed" | "none"
    pub checks_status: String,
    /// Whether the MR can be cleanly merged. None = unknown.
    pub mergeable:     Option<bool>,
    /// "github" | "gitlab"
    pub provider:      String,
    pub comments_count: u32,
    /// Squash commits on merge (set at creation / from API).
    #[serde(default)]
    pub squash:        bool,
    /// Delete source branch after merge.
    #[serde(default)]
    pub delete_branch: bool,
    /// SHA of the commit that was created on the target branch when this MR/PR
    /// was merged (squash commit SHA for squash merges, merge commit SHA for
    /// regular merges).  None for open/closed-without-merge MRs.
    #[serde(default)]
    pub merge_commit_sha: Option<String>,
    /// SHA of the source branch tip at the time of merge (head.sha).
    #[serde(default)]
    pub head_sha: String,
    /// SHA of the target branch tip just before the merge (base.sha).
    #[serde(default)]
    pub base_sha: String,
    /// Auto-merge is currently armed on this PR/MR — it will merge itself when
    /// required checks pass (GitHub) / the pipeline succeeds (GitLab).
    /// While armed, the manual merge button + squash/delete-branch flags in
    /// the detail modal are suppressed; a "Disable auto-merge" affordance is
    /// shown instead.
    #[serde(default)]
    pub auto_merge_enabled: bool,
}

/// Lightweight hint for cross-referencing merged PRs/MRs in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedMrHint {
    /// Name of the source (feature) branch.
    pub source_branch:    String,
    /// SHA of the merge/squash commit created on the target branch.
    /// May not exist locally if the user hasn't fetched yet.
    pub merge_commit_sha: String,
    /// SHA of the feature branch tip at the time of merge (head.sha).
    /// Always present in the local graph.
    pub head_sha:         String,
    /// SHA of the target branch tip just before the merge (base.sha).
    /// Can be used as a fallback anchor when merge_commit_sha isn't local yet.
    pub base_sha:         String,
}

/// Parameters for creating a new PR/MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMrParams {
    pub title:          String,
    pub description:    Option<String>,
    pub source_branch:  String,
    pub target_branch:  String,
    pub is_draft:       bool,
    pub labels:         Vec<String>,
    /// Squash commits on merge (applied at merge-time for GitHub; set on GitLab at creation).
    #[serde(default)]
    pub squash:         bool,
    /// Delete the source branch after a successful merge.
    #[serde(default)]
    pub delete_branch:  bool,
    /// Request auto-merge once checks pass (GitHub) / pipeline succeeds (GitLab).
    /// Merge/delete-branch options remain editable later from the detail modal.
    #[serde(default)]
    pub auto_merge:     bool,
}

/// Full detail for the detail modal (MR + comments + activity events + checks).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MrDetail {
    pub mr:       MergeRequest,
    pub comments: Vec<MrComment>,
    /// Timeline events (state changes, label edits, assignments, etc.).
    /// Empty when the provider/API doesn't surface them — the frontend
    /// handles that gracefully by hiding the Activity filter chip.
    #[serde(default)]
    pub events:   Vec<MrEvent>,
    pub checks:   Vec<MrCheck>,
}

/// A single changed file in a PR / MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFileDiff {
    pub filename:  String,
    /// "added" | "modified" | "removed" | "renamed"
    pub status:    String,
    pub additions: u32,
    pub deletions: u32,
    pub patch:     Option<String>,
}

/// A commit belonging to a PR / MR.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrCommit {
    pub sha:     String,
    /// First line of the commit message.
    pub message: String,
    pub author:  String,
    pub date:    String,
    pub web_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Provider resolution (delegates to ci_client)
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub fn provider_from_remotes(
    remotes: &[(String, String)],
) -> Option<CiProviderInfo> {
    detect_from_remotes(remotes)
}

// ---------------------------------------------------------------------------
// GitHub Pull Requests
// ---------------------------------------------------------------------------

pub async fn list_github_prs(
    owner: &str,
    repo:  &str,
    token: &str,
    state: &str, // "open" | "closed" | "all"
) -> Result<Vec<MergeRequest>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls?state={state}&per_page=50&sort=updated&direction=desc"
    );
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GitHub API request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub API {status}: {body}")));
    }

    let body = resp.text().await
        .map_err(|e| AppError::Other(format!("GitHub PR body read error: {e}")))?;
    let prs: Vec<GhPr> = serde_json::from_str(&body)
        .map_err(|e| AppError::Other(format!("GitHub PR parse error: {e} — body: {}", &body[..body.len().min(300)])))?;

    Ok(prs.into_iter().map(|p| github_pr_to_mr(p, owner, repo)).collect())
}

pub async fn get_github_pr(
    owner:  &str,
    repo:   &str,
    number: u64,
    token:  &str,
) -> Result<MrDetail> {
    let client = reqwest::Client::new();

    // Fetch PR itself
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let pr_resp = client.get(&pr_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub PR get failed: {e}")))?;

    if !pr_resp.status().is_success() {
        let s = pr_resp.status();
        let b = pr_resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub API {s}: {b}")));
    }
    let pr: GhPrDetail = pr_resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub PR detail parse: {e}")))?;

    let mut mr = github_pr_detail_to_mr(pr, owner, repo);

    // Fetch issue comments (general comments on the PR thread)
    let comments_url = format!("https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments?per_page=50");
    let comments_resp = client.get(&comments_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await;

    let comments: Vec<MrComment> = match comments_resp {
        Ok(r) if r.status().is_success() => {
            let raw: Vec<GhComment> = r.json().await.unwrap_or_default();
            raw.into_iter().map(gh_comment_to_mr).collect()
        }
        _ => vec![],
    };

    mr.comments = comments;

    // Fetch issue events (label/assign/state/etc.) — separate endpoint from
    // comments. Failures are non-fatal: an empty events list just hides the
    // Activity filter on the frontend.
    let events_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/issues/{number}/events?per_page=100"
    );
    let events_resp = client.get(&events_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await;

    let events: Vec<MrEvent> = match events_resp {
        Ok(r) if r.status().is_success() => {
            let raw: Vec<GhEvent> = r.json().await.unwrap_or_default();
            raw.into_iter().filter_map(gh_event_to_mr).collect()
        }
        _ => vec![],
    };
    mr.events = events;

    // Checks left empty; checks_status on the MR itself shows the summary.
    mr.checks   = vec![];
    Ok(mr)
}

/// Returns `(MergeRequest, Option<node_id>)`.  `node_id` is the GraphQL Relay
/// ID of the PR — required to enable auto-merge via the GraphQL mutation.
pub async fn create_github_pr(
    owner:  &str,
    repo:   &str,
    params: &CreateMrParams,
    token:  &str,
) -> Result<(MergeRequest, Option<String>)> {
    let body = serde_json::json!({
        "title": params.title,
        "body":  params.description.as_deref().unwrap_or(""),
        "head":  params.source_branch,
        "base":  params.target_branch,
        "draft": params.is_draft,
    });
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://api.github.com/repos/{owner}/{repo}/pulls"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub create PR failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(github_create_pr_error(s, b, params));
    }
    let raw: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub create PR parse: {e}")))?;
    let node_id = raw.get("node_id").and_then(|v| v.as_str()).map(ToOwned::to_owned);
    let pr: GhPr = serde_json::from_value(raw)
        .map_err(|e| AppError::Other(format!("GitHub create PR parse: {e}")))?;
    let mut mr = github_pr_to_mr(pr, owner, repo);
    mr.squash        = params.squash;
    mr.delete_branch = params.delete_branch;
    Ok((mr, node_id))
}

/// Map a GitHub "create PR" failure to a human-readable message.  GitHub
/// 422s are particularly cryptic ("head: invalid") without context — we
/// translate the common shapes and fall back to the raw body only when we
/// don't recognise the error.
fn github_create_pr_error(
    status: reqwest::StatusCode,
    body:   String,
    params: &CreateMrParams,
) -> AppError {
    let parsed: std::result::Result<serde_json::Value, _> = serde_json::from_str(&body);
    if let Ok(v) = parsed {
        let message = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
        if let Some(errs) = v.get("errors").and_then(|e| e.as_array()) {
            for e in errs {
                let field = e.get("field").and_then(|f| f.as_str()).unwrap_or("");
                let code  = e.get("code").and_then(|c| c.as_str()).unwrap_or("");
                match (field, code) {
                    ("head", "invalid") => return AppError::Other(format!(
                        "Source branch '{}' was not found on the GitHub remote. \
                         Push it to origin first, then try again.",
                        params.source_branch,
                    )),
                    ("base", "invalid") => return AppError::Other(format!(
                        "Target branch '{}' does not exist on the GitHub remote.",
                        params.target_branch,
                    )),
                    (_, "missing_field") => return AppError::Other(format!(
                        "GitHub rejected the PR: required field '{field}' is missing.",
                    )),
                    (_, "custom") => {
                        if let Some(m) = e.get("message").and_then(|m| m.as_str()) {
                            return AppError::Other(format!("GitHub: {m}"));
                        }
                    }
                    _ => {}
                }
            }
        }
        let lower = message.to_lowercase();
        if lower.contains("no commits between") {
            return AppError::Other(format!(
                "No commits between '{}' and '{}' — nothing to merge yet. \
                 Commit and push your changes first.",
                params.target_branch, params.source_branch,
            ));
        }
        if lower.contains("a pull request already exists") {
            return AppError::Other(format!(
                "A pull request already exists for '{}' → '{}'.",
                params.source_branch, params.target_branch,
            ));
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return AppError::Other(
                "GitHub refused the request — check your token permissions in \
                 Settings → Git & Integrations.".into(),
            );
        }
        if !message.is_empty() {
            return AppError::Other(format!("GitHub: {message}"));
        }
    }
    // Unknown shape — surface status + trimmed body so the issue can still be diagnosed.
    let trimmed: String = body.chars().take(300).collect();
    AppError::Other(format!("GitHub create PR {status}: {trimmed}"))
}

/// Enable auto-merge on a GitHub PR via GraphQL.
/// `merge_method` is one of `"MERGE" | "SQUASH" | "REBASE"`.
/// Fails when the repo doesn't have branch protection rules requiring reviews
/// or checks (auto-merge is gated on those).  Error message is bubbled up so
/// the caller can surface it to the user.
pub async fn enable_github_auto_merge(
    pr_node_id:   &str,
    merge_method: &str,
    token:        &str,
) -> Result<()> {
    let method = match merge_method.to_uppercase().as_str() {
        "SQUASH" => "SQUASH",
        "REBASE" => "REBASE",
        _        => "MERGE",
    };
    let query = "mutation($id: ID!, $m: PullRequestMergeMethod!) { \
        enablePullRequestAutoMerge(input: { pullRequestId: $id, mergeMethod: $m }) { \
            pullRequest { autoMergeRequest { enabledAt } } \
        } }";
    let body = serde_json::json!({ "query": query, "variables": { "id": pr_node_id, "m": method } });

    let resp = reqwest::Client::new()
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub auto-merge request failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub auto-merge {s}: {b}")));
    }

    let data: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub auto-merge parse: {e}")))?;
    if let Some(errs) = data.get("errors") {
        // Extract the first error message for a concise user-facing notice.
        let msg = errs.get(0)
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&errs.to_string())
            .to_string();
        return Err(AppError::Other(msg));
    }
    Ok(())
}

pub async fn merge_github_pr(
    owner:        &str,
    repo:         &str,
    number:       u64,
    merge_method: &str, // "merge" | "squash" | "rebase"
    token:        &str,
) -> Result<()> {
    let body = serde_json::json!({ "merge_method": merge_method });
    let client = reqwest::Client::new();
    let resp = client
        .put(format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}/merge"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub merge PR failed: {e}")))?;

    if resp.status().is_success() || resp.status().as_u16() == 200 {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub merge PR {s}: {b}")))
}

pub async fn update_github_pr_state(
    owner:  &str,
    repo:   &str,
    number: u64,
    state:  &str, // "open" | "closed"
    token:  &str,
) -> Result<()> {
    let body = serde_json::json!({ "state": state });
    let client = reqwest::Client::new();
    let resp = client
        .patch(format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub update PR failed: {e}")))?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub update PR {s}: {b}")))
}

pub async fn add_github_pr_comment(
    owner:  &str,
    repo:   &str,
    number: u64,
    body:   &str,
    token:  &str,
) -> Result<()> {
    let payload = serde_json::json!({ "body": body });
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&payload)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub add comment failed: {e}")))?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitHub add comment {s}: {b}")))
}

// ---------------------------------------------------------------------------
// GitLab Merge Requests
// ---------------------------------------------------------------------------

pub async fn list_gitlab_mrs(
    project_path: &str,
    base_url:     &str,
    token:        &str,
    state:        &str, // "opened" | "closed" | "merged" | "all"
) -> Result<Vec<MergeRequest>> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/merge_requests?state={state}&per_page=50&order_by=updated_at&sort=desc"
    );
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab API {s}: {b}")));
    }

    let body = resp.text().await
        .map_err(|e| AppError::Other(format!("GitLab MR body read error: {e}")))?;
    let mrs: Vec<GlMr> = serde_json::from_str(&body)
        .map_err(|e| AppError::Other(format!("GitLab MR parse: {e} — body snippet: {}", &body[..body.len().min(400)])))?;
    Ok(mrs.into_iter().map(gitlab_mr_to_mr).collect())
}

pub async fn get_gitlab_mr(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    token:        &str,
) -> Result<MrDetail> {
    let encoded = percent_encode_slash(project_path);
    let client  = reqwest::Client::new();

    let url = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab API {s}: {b}")));
    }
    let mr: GlMr = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab MR detail parse: {e}")))?;
    let mr_converted = gitlab_mr_to_mr(mr);

    // Fetch notes
    let notes_url = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}/notes?sort=asc&order_by=created_at&per_page=50");
    let notes_resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&notes_url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await;

    let (comments, events): (Vec<MrComment>, Vec<MrEvent>) = match notes_resp {
        Ok(r) if r.status().is_success() => {
            let raw: Vec<GlNote> = r.json().await.unwrap_or_default();
            // Partition: user-authored notes → comments, system notes → events.
            let mut comments = Vec::new();
            let mut events   = Vec::new();
            for n in raw {
                if n.system {
                    events.push(gl_system_note_to_event(n));
                } else {
                    comments.push(gl_note_to_comment(n));
                }
            }
            (comments, events)
        }
        _ => (vec![], vec![]),
    };

    Ok(MrDetail {
        mr:       mr_converted,
        comments,
        events,
        checks:   vec![],
    })
}

pub async fn create_gitlab_mr(
    project_path: &str,
    base_url:     &str,
    params:       &CreateMrParams,
    token:        &str,
) -> Result<MergeRequest> {
    let encoded = percent_encode_slash(project_path);
    let mut body = serde_json::json!({
        "title":                    params.title,
        "description":              params.description.as_deref().unwrap_or(""),
        "source_branch":            params.source_branch,
        "target_branch":            params.target_branch,
        "squash":                   params.squash,
        "should_remove_source_branch": params.delete_branch,
    });
    if params.is_draft {
        body["title"] = serde_json::Value::String(format!("Draft: {}", params.title));
    }
    if !params.labels.is_empty() {
        body["labels"] = serde_json::Value::String(params.labels.join(","));
    }

    let client = reqwest::Client::new();
    let url_create = format!("{base_url}/api/v4/projects/{encoded}/merge_requests");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .post(&url_create)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(gitlab_create_mr_error(s, b, params));
    }
    let mr: GlMr = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab create MR parse: {e}")))?;
    Ok(gitlab_mr_to_mr(mr))
}

/// Map a GitLab "create MR" failure to a human-readable message.  GitLab
/// returns errors as `{"message": {"field": ["reason", ...]}}` or
/// `{"message": "..."}` depending on the validation layer.
fn gitlab_create_mr_error(
    status: reqwest::StatusCode,
    body:   String,
    params: &CreateMrParams,
) -> AppError {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        let msg_node = v.get("message").or_else(|| v.get("error"));
        // Flat string message.
        if let Some(s) = msg_node.and_then(|m| m.as_str()) {
            let low = s.to_lowercase();
            if low.contains("source") && low.contains("does not exist") {
                return AppError::Other(format!(
                    "Source branch '{}' was not found on the GitLab remote. \
                     Push it to origin first, then try again.",
                    params.source_branch,
                ));
            }
            if low.contains("target") && low.contains("does not exist") {
                return AppError::Other(format!(
                    "Target branch '{}' does not exist on the GitLab remote.",
                    params.target_branch,
                ));
            }
            if low.contains("another open merge request already exists")
                || low.contains("already exists")
            {
                return AppError::Other(format!(
                    "A merge request already exists for '{}' → '{}'.",
                    params.source_branch, params.target_branch,
                ));
            }
            if !s.is_empty() {
                return AppError::Other(format!("GitLab: {s}"));
            }
        }
        // Nested error object: `{"message": {"source_branch": ["can't be blank"]}}`.
        if let Some(obj) = msg_node.and_then(|m| m.as_object()) {
            let source_err = obj.get("source_branch")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str());
            if let Some(err) = source_err {
                return AppError::Other(format!(
                    "Source branch '{}' — {err}. \
                     If it hasn't been pushed yet, push it to origin first.",
                    params.source_branch,
                ));
            }
            let target_err = obj.get("target_branch")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str());
            if let Some(err) = target_err {
                return AppError::Other(format!(
                    "Target branch '{}' — {err}.",
                    params.target_branch,
                ));
            }
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return AppError::Other(
                "GitLab refused the request — check your token permissions in \
                 Settings → Git & Integrations.".into(),
            );
        }
    }
    let trimmed: String = body.chars().take(300).collect();
    AppError::Other(format!("GitLab create MR {status}: {trimmed}"))
}

pub async fn merge_gitlab_mr(
    project_path:  &str,
    base_url:      &str,
    iid:           u64,
    squash:        bool,
    delete_branch: bool,
    token:         &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let body    = serde_json::json!({
        "squash":                      squash,
        "should_remove_source_branch": delete_branch,
    });
    let client  = reqwest::Client::new();
    let url_merge = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}/merge");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .put(&url_merge)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab merge MR {s}: {b}")))
}

/// Poll the MR until GitLab finishes computing `merge_status` (it starts as
/// `checking`/`unchecked` right after creation). Returns once the status is
/// resolved, or after the timeout — callers should still attempt the merge
/// either way so transient API hiccups don't block the user.
pub async fn wait_gitlab_merge_status_ready(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    token:        &str,
) {
    #[derive(Deserialize)]
    struct StatusOnly { merge_status: Option<String> }

    let encoded = percent_encode_slash(project_path);
    let url     = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}");
    let client  = reqwest::Client::new();

    let delays_ms = [400u64, 600, 800, 1200, 1500, 1500];
    for delay in delays_ms {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
            |tok| client.get(&url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("User-Agent", "arbor-git-gui/1.0"),
            base_url,
            token,
        ).await;
        let Ok(r) = resp else { continue };
        if !r.status().is_success() { continue; }
        let Ok(s) = r.json::<StatusOnly>().await else { continue };
        match s.merge_status.as_deref() {
            Some("checking") | Some("unchecked") | None => continue,
            _ => return,
        }
    }
}

/// Disable auto-merge on a GitHub PR. Looks up the PR's GraphQL node ID first
/// (REST `/pulls/{n}` carries `node_id`), then sends the `disablePullRequestAutoMerge`
/// mutation. The mutation is a no-op when auto-merge isn't currently armed.
pub async fn disable_github_auto_merge(
    owner:  &str,
    repo:   &str,
    number: u64,
    token:  &str,
) -> Result<()> {
    let client = reqwest::Client::new();

    // Resolve node_id via REST so we don't need a second auth flow for GraphQL.
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let resp = client.get(&pr_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub PR lookup failed: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub PR lookup {s}: {b}")));
    }
    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub PR parse: {e}")))?;
    let node_id = v.get("node_id").and_then(|n| n.as_str())
        .ok_or_else(|| AppError::Other("GitHub PR response missing node_id".into()))?;

    let query = "mutation($id: ID!) { \
        disablePullRequestAutoMerge(input: { pullRequestId: $id }) { \
            pullRequest { number } \
        } }";
    let body = serde_json::json!({ "query": query, "variables": { "id": node_id } });

    let resp = client
        .post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub disable auto-merge failed: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub disable auto-merge {s}: {b}")));
    }
    let data: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub disable auto-merge parse: {e}")))?;
    if let Some(errs) = data.get("errors") {
        let msg = errs.get(0)
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&errs.to_string())
            .to_string();
        return Err(AppError::Other(msg));
    }
    Ok(())
}

/// Enable "merge when pipeline succeeds" on a GitLab MR.
/// When no pipeline exists this endpoint merges immediately, so upstream code
/// should only call this when the MR has CI configured.  Any failure is
/// bubbled up as an error so the caller can notify the user.
pub async fn enable_gitlab_auto_merge(
    project_path:  &str,
    base_url:      &str,
    iid:           u64,
    squash:        bool,
    delete_branch: bool,
    token:         &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let body = serde_json::json!({
        "merge_when_pipeline_succeeds": true,
        "squash":                       squash,
        "should_remove_source_branch":  delete_branch,
    });
    let client = reqwest::Client::new();
    let url = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}/merge");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .put(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab auto-merge {s}: {b}")))
}

/// Cancel "merge when pipeline succeeds" on a GitLab MR.
/// Idempotent — the endpoint returns 200 even if MWPS isn't currently armed.
pub async fn disable_gitlab_auto_merge(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    token:        &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let client  = reqwest::Client::new();
    let url = format!(
        "{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}/cancel_merge_when_pipeline_succeeds"
    );
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .header("Content-Length", "0"),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab cancel-MWPS {s}: {b}")))
}

// ---------------------------------------------------------------------------
// Auto-merge capability detection
// ---------------------------------------------------------------------------

/// Per-repo capability flags surfaced to the Create-MR modal so it can hide
/// or disable options the upstream provider rejects.
///
/// Currently scoped to auto-merge — GitHub gates this on the repo-level
/// `Allow auto-merge` setting, and arming MWPS on GitLab requires the
/// project to have CI jobs enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrCapabilities {
    /// `true` when arming auto-merge / MWPS at PR/MR creation time is
    /// expected to succeed.  Defaults to `true` on any detection failure
    /// (missing token, network error, …) so the user can still try.
    pub auto_merge_supported: bool,
    /// User-facing reason when `auto_merge_supported = false` — used as
    /// the disabled-checkbox tooltip.
    pub auto_merge_reason:    Option<String>,
}

impl Default for MrCapabilities {
    fn default() -> Self {
        Self { auto_merge_supported: true, auto_merge_reason: None }
    }
}

/// Query the GitHub repo endpoint and return the value of `allow_auto_merge`.
/// Falls back to `true` when the field is absent (e.g. unauthenticated /
/// non-admin response) so the option stays available — the create call will
/// surface a notification if it ultimately fails.
pub async fn fetch_github_auto_merge_allowed(
    owner: &str,
    repo:  &str,
    token: &str,
) -> Result<bool> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub repo fetch failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub repo fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub repo parse: {e}")))?;
    // Treat a missing field as "unknown → allow"; only an explicit false disables.
    Ok(v.get("allow_auto_merge").and_then(|b| b.as_bool()).unwrap_or(true))
}

/// Query the GitLab project endpoint and check whether MWPS can be armed.
/// We approximate "MWPS supported" with `jobs_enabled` — a project without
/// CI cannot have a pipeline to wait on, so MWPS would degrade to an
/// immediate merge or a 405.
pub async fn fetch_gitlab_mwps_supported(
    project_path: &str,
    base_url:     &str,
    token:        &str,
) -> Result<bool> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{base_url}/api/v4/projects/{encoded}");
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab project fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab project parse: {e}")))?;
    Ok(v.get("jobs_enabled").and_then(|b| b.as_bool()).unwrap_or(true))
}

// ---------------------------------------------------------------------------
// MR/PR feature availability probe
// ---------------------------------------------------------------------------

/// Whether the remote repository accepts merge/pull requests at all.
///
/// Drives the sidebar EmptyState + Command-Palette gating so a repo with
/// MRs disabled doesn't surface broken actions or 404s.  Defaults to
/// `enabled = true` on any probe failure (permissive — the user can still
/// try and the failing call will surface a normal error).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFeatureStatus {
    pub enabled: bool,
    /// User-facing explanation when `enabled = false`.
    pub reason:  Option<String>,
}

impl Default for MrFeatureStatus {
    fn default() -> Self { Self { enabled: true, reason: None } }
}

/// GitHub probe: archived or disabled repos cannot accept new PRs and the
/// `/pulls` endpoint may 404 on certain configurations.
///
/// TODO: GitHub has no granular `has_pull_requests` flag. Edge cases we
/// don't yet catch: fork-mirrors whose upstream blocks PRs, repos with
/// branch-protection forbidding PRs entirely. If 404 keeps surfacing in
/// the wild, add a `list_mrs(per_page=1)` fallback here.
pub async fn fetch_github_pr_feature_enabled(
    owner: &str,
    repo:  &str,
    token: &str,
) -> Result<MrFeatureStatus> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub repo fetch failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub repo fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub repo parse: {e}")))?;
    let archived = v.get("archived").and_then(|b| b.as_bool()).unwrap_or(false);
    let disabled = v.get("disabled").and_then(|b| b.as_bool()).unwrap_or(false);
    if disabled {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason:  Some("This repository is disabled on GitHub.".into()),
        });
    }
    if archived {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason:  Some("This repository is archived — new pull requests cannot be opened.".into()),
        });
    }
    Ok(MrFeatureStatus::default())
}

/// GitLab probe: `merge_requests_access_level = "disabled"` means the
/// MR feature has been turned off in project settings, so every MR call
/// returns 404.
pub async fn fetch_gitlab_mr_feature_enabled(
    project_path: &str,
    base_url:     &str,
    token:        &str,
) -> Result<MrFeatureStatus> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{base_url}/api/v4/projects/{encoded}");
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base_url,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab project fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab project parse: {e}")))?;
    let access = v.get("merge_requests_access_level")
        .and_then(|s| s.as_str())
        .unwrap_or("enabled");
    if access == "disabled" {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason:  Some("Merge requests are disabled in this project's settings on GitLab.".into()),
        });
    }
    Ok(MrFeatureStatus::default())
}

pub async fn update_gitlab_mr_state(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    event:        &str, // "close" | "reopen"
    token:        &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let body    = serde_json::json!({ "state_event": event });
    let client  = reqwest::Client::new();
    let url_state = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .put(&url_state)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab update MR {s}: {b}")))
}

/// Mark a GitHub PR as ready for review (removes draft status).
/// GitHub's REST API does NOT support converting draft→ready; requires GraphQL.
pub async fn mark_github_pr_ready(
    owner:  &str,
    repo:   &str,
    number: u64,
    token:  &str,
) -> Result<()> {
    let client = reqwest::Client::new();

    // Step 1: fetch the PR node_id (required by GraphQL).
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let pr_resp = client.get(&pr_url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub mark ready (fetch node_id) failed: {e}")))?;

    if !pr_resp.status().is_success() {
        let s = pr_resp.status();
        let b = pr_resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub mark ready (fetch node_id) {s}: {b}")));
    }
    let pr_data: serde_json::Value = pr_resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub mark ready (parse node_id): {e}")))?;
    let node_id = pr_data["node_id"].as_str()
        .ok_or_else(|| AppError::Other("GitHub mark ready: node_id missing from PR response".into()))?
        .to_owned();

    // Step 2: GraphQL mutation markPullRequestReadyForReview.
    let query = "mutation MarkReady($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { pullRequest { isDraft } } }";
    let gql_body = serde_json::json!({ "query": query, "variables": { "id": node_id } });

    let gql_resp = client.post("https://api.github.com/graphql")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "arbor-git-gui/1.0")
        .json(&gql_body)
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub mark ready (GraphQL) failed: {e}")))?;

    if !gql_resp.status().is_success() {
        let s = gql_resp.status();
        let b = gql_resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub mark ready (GraphQL) {s}: {b}")));
    }

    // GraphQL always returns 200; check for errors in the response body.
    let gql_data: serde_json::Value = gql_resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub mark ready (GraphQL parse): {e}")))?;
    if let Some(errors) = gql_data.get("errors") {
        let msg = errors.to_string();
        return Err(AppError::Other(format!("GitHub mark ready (GraphQL errors): {msg}")));
    }

    Ok(())
}

/// Mark a GitLab MR as ready for review (removes Draft prefix).
pub async fn mark_gitlab_mr_ready(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    token:        &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    // GitLab API supports draft:false directly since v14.x
    let body = serde_json::json!({ "draft": false });
    let client = reqwest::Client::new();
    let url_ready = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .put(&url_ready)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab mark ready {s}: {b}")))
}

// /// Mark a GitHub PR as ready for review (removes draft status).
// /// GitHub's REST API does NOT support converting draft→ready; requires GraphQL.
// pub async fn mark_github_pr_ready(
//     owner:  &str,
//     repo:   &str,
//     number: u64,
//     token:  &str,
// ) -> Result<()> {
//     let client = reqwest::Client::new();

//     // Step 1: fetch the PR node_id (required by GraphQL).
//     let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
//     let pr_resp = client.get(&pr_url)
//         .header("Authorization", format!("Bearer {token}"))
//         .header("Accept", "application/vnd.github+json")
//         .header("X-GitHub-Api-Version", "2022-11-28")
//         .header("User-Agent", "arbor-git-gui/1.0")
//         .send().await
//         .map_err(|e| AppError::Other(format!("GitHub mark ready (fetch node_id) failed: {e}")))?;

//     if !pr_resp.status().is_success() {
//         let s = pr_resp.status();
//         let b = pr_resp.text().await.unwrap_or_default();
//         return Err(AppError::Other(format!("GitHub mark ready (fetch node_id) {s}: {b}")));
//     }
//     let pr_data: serde_json::Value = pr_resp.json().await
//         .map_err(|e| AppError::Other(format!("GitHub mark ready (parse node_id): {e}")))?;
//     let node_id = pr_data["node_id"].as_str()
//         .ok_or_else(|| AppError::Other("GitHub mark ready: node_id missing from PR response".into()))?
//         .to_owned();

//     // Step 2: GraphQL mutation markPullRequestReadyForReview.
//     let query = "mutation MarkReady($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { pullRequest { isDraft } } }";
//     let gql_body = serde_json::json!({ "query": query, "variables": { "id": node_id } });

//     let gql_resp = client.post("https://api.github.com/graphql")
//         .header("Authorization", format!("Bearer {token}"))
//         .header("Content-Type", "application/json")
//         .header("User-Agent", "arbor-git-gui/1.0")
//         .json(&gql_body)
//         .send().await
//         .map_err(|e| AppError::Other(format!("GitHub mark ready (GraphQL) failed: {e}")))?;

//     if !gql_resp.status().is_success() {
//         let s = gql_resp.status();
//         let b = gql_resp.text().await.unwrap_or_default();
//         return Err(AppError::Other(format!("GitHub mark ready (GraphQL) {s}: {b}")));
//     }

//     // GraphQL always returns 200; check for errors in the response body.
//     let gql_data: serde_json::Value = gql_resp.json().await
//         .map_err(|e| AppError::Other(format!("GitHub mark ready (GraphQL parse): {e}")))?;
//     if let Some(errors) = gql_data.get("errors") {
//         let msg = errors.to_string();
//         return Err(AppError::Other(format!("GitHub mark ready (GraphQL errors): {msg}")));
//     }

//     Ok(())
// }

// /// Mark a GitLab MR as ready for review (removes Draft prefix).
// pub async fn mark_gitlab_mr_ready(
//     project_path: &str,
//     base_url:     &str,
//     iid:          u64,
//     token:        &str,
// ) -> Result<()> {
//     let encoded = percent_encode_slash(project_path);
//     // GitLab API supports draft:false directly since v14.x
//     let body = serde_json::json!({ "draft": false });
//     let client = reqwest::Client::new();
//     let resp = client
//         .put(format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}"))
//         .header("PRIVATE-TOKEN", token)
//         .header("User-Agent", "arbor-git-gui/1.0")
//         .json(&body)
//         .send().await
//         .map_err(|e| AppError::Other(format!("GitLab mark ready failed: {e}")))?;

//     if resp.status().is_success() { return Ok(()); }
//     let s = resp.status();
//     let b = resp.text().await.unwrap_or_default();
//     Err(AppError::Other(format!("GitLab mark ready {s}: {b}")))
// }

pub async fn add_gitlab_mr_note(
    project_path: &str,
    base_url:     &str,
    iid:          u64,
    body:         &str,
    token:        &str,
) -> Result<()> {
    let encoded = percent_encode_slash(project_path);
    let payload = serde_json::json!({ "body": body });
    let client  = reqwest::Client::new();
    let url_note = format!("{base_url}/api/v4/projects/{encoded}/merge_requests/{iid}/notes");
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client
            .post(&url_note)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&payload),
        base_url,
        token,
    ).await?;

    if resp.status().is_success() { return Ok(()); }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(AppError::Other(format!("GitLab add note {s}: {b}")))
}

// ---------------------------------------------------------------------------
// Internal GitHub raw types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhPr {
    number:       u64,
    title:        String,
    #[serde(default)]
    body:         Option<String>,
    state:        String,
    draft:        Option<bool>,
    #[serde(default)]
    merged_at:    Option<String>,
    #[serde(default)]
    merge_commit_sha: Option<String>,
    user:         GhUser,
    head:         GhRef,
    base:         GhRef,
    html_url:     String,
    created_at:   String,
    updated_at:   String,
    #[serde(default)]
    labels:       Vec<GhLabel>,
    #[serde(default)]
    assignees:    Vec<GhUser>,
    #[serde(default)]
    requested_reviewers: Vec<GhUser>,
    #[serde(default)]
    comments:     u32,
    /// GitHub returns an object (`{ enabled_by, merge_method, commit_title, … }`)
    /// when auto-merge is armed, `null` otherwise. We only care about presence.
    #[serde(default)]
    auto_merge:   Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GhPrDetail {
    number:       u64,
    title:        String,
    #[serde(default)]
    body:         Option<String>,
    state:        String,
    draft:        Option<bool>,
    merged:       Option<bool>,
    #[serde(default)]
    merge_commit_sha: Option<String>,
    user:         GhUser,
    head:         GhRef,
    base:         GhRef,
    html_url:     String,
    created_at:   String,
    updated_at:   String,
    #[serde(default)]
    labels:       Vec<GhLabel>,
    #[serde(default)]
    assignees:    Vec<GhUser>,
    #[serde(default)]
    requested_reviewers: Vec<GhUser>,
    #[serde(default)]
    comments:     u32,
    mergeable:    Option<bool>,
    /// Present + non-null when auto-merge is armed. See `GhPr.auto_merge`.
    #[serde(default)]
    auto_merge:   Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GhUser {
    login:      String,
    #[serde(default)]
    name:       Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GhRef {
    #[serde(rename = "ref")]
    ref_name: String,
    sha:      String,
}

#[derive(Deserialize)]
struct GhLabel {
    name:  String,
    color: String,
}

#[derive(Deserialize)]
struct GhComment {
    id:         i64,
    user:       GhUser,
    body:       String,
    created_at: String,
}

/// GitHub `/issues/{n}/events` payload — a heterogeneous list of timeline
/// events. We parse only the fields we render; everything else is ignored.
#[derive(Deserialize)]
struct GhEvent {
    id:         i64,
    /// "labeled" | "unlabeled" | "assigned" | "unassigned" | "closed" |
    /// "reopened" | "merged" | "renamed" | "head_ref_force_pushed" |
    /// "head_ref_deleted" | "head_ref_restored" | "review_requested" |
    /// "review_request_removed" | "convert_to_draft" | "ready_for_review" |
    /// "auto_merge_enabled" | "auto_merge_disabled" | "milestoned" |
    /// "demilestoned" | "locked" | "unlocked" | … (many more, fall through to system)
    event:      String,
    #[serde(default)]
    actor:      Option<GhUser>,
    created_at: String,
    #[serde(default)]
    label:      Option<GhLabel>,
    #[serde(default)]
    assignee:   Option<GhUser>,
    #[serde(default)]
    requested_reviewer: Option<GhUser>,
    #[serde(default)]
    rename:     Option<GhRename>,
}

#[derive(Deserialize)]
struct GhRename {
    from: String,
    to:   String,
}

fn github_pr_to_mr(p: GhPr, _owner: &str, _repo: &str) -> MergeRequest {
    let state = match p.state.as_str() {
        "closed" if p.merged_at.is_some() => MrState::Merged,
        "closed"                          => MrState::Closed,
        _                                 => MrState::Open,
    };
    let auto_merge_enabled = p.auto_merge.as_ref().map_or(false, |v| !v.is_null());
    MergeRequest {
        number:        p.number,
        title:         p.title,
        description:   p.body.unwrap_or_default(),
        state,
        is_draft:      p.draft.unwrap_or(false),
        author:        gh_user_to_mr(p.user),
        source_branch: p.head.ref_name,
        target_branch: p.base.ref_name,
        web_url:       p.html_url,
        created_at:    p.created_at,
        updated_at:    p.updated_at,
        labels:        p.labels.into_iter().map(|l| MrLabel { name: l.name, color: l.color }).collect(),
        assignees:     p.assignees.into_iter().map(gh_user_to_mr).collect(),
        reviewers:     p.requested_reviewers.into_iter().map(gh_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable:     None,
        provider:      "github".into(),
        comments_count: p.comments,
        squash:        false,
        delete_branch: false,
        merge_commit_sha: p.merge_commit_sha,
        head_sha:         p.head.sha,
        base_sha:         p.base.sha,
        auto_merge_enabled,
    }
}

fn github_pr_detail_to_mr(p: GhPrDetail, _owner: &str, _repo: &str) -> MrDetail {
    let state = match (p.state.as_str(), p.merged) {
        (_, Some(true)) => MrState::Merged,
        ("closed", _)   => MrState::Closed,
        _               => MrState::Open,
    };
    let auto_merge_enabled = p.auto_merge.as_ref().map_or(false, |v| !v.is_null());
    let mr = MergeRequest {
        number:        p.number,
        title:         p.title,
        description:   p.body.unwrap_or_default(),
        state,
        is_draft:      p.draft.unwrap_or(false),
        author:        gh_user_to_mr(p.user),
        source_branch: p.head.ref_name,
        target_branch: p.base.ref_name,
        web_url:       p.html_url,
        created_at:    p.created_at,
        updated_at:    p.updated_at,
        labels:        p.labels.into_iter().map(|l| MrLabel { name: l.name, color: l.color }).collect(),
        assignees:     p.assignees.into_iter().map(gh_user_to_mr).collect(),
        reviewers:     p.requested_reviewers.into_iter().map(gh_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable:     p.mergeable,
        provider:      "github".into(),
        comments_count: p.comments,
        squash:        false,
        delete_branch: false,
        merge_commit_sha: p.merge_commit_sha,
        head_sha:         p.head.sha,
        base_sha:         p.base.sha,
        auto_merge_enabled,
    };
    MrDetail { mr, comments: vec![], events: vec![], checks: vec![] }
}

fn gh_user_to_mr(u: GhUser) -> MrUser {
    MrUser {
        login:        u.login.clone(),
        display_name: u.name.unwrap_or(u.login),
        avatar_url:   u.avatar_url,
    }
}

fn gh_comment_to_mr(c: GhComment) -> MrComment {
    let is_bot = is_bot_login(&c.user.login);
    MrComment {
        id:         c.id.to_string(),
        author:     gh_user_to_mr(c.user),
        body:       c.body,
        created_at: c.created_at,
        is_bot,
    }
}

/// Convert a GitHub issue/PR event into an `MrEvent`. Returns None for
/// event types we don't surface (e.g. `subscribed`, `mentioned`, `referenced`),
/// which are mostly noise on the timeline.
fn gh_event_to_mr(e: GhEvent) -> Option<MrEvent> {
    // The "ghost" actor is GitHub's placeholder for deleted users.
    let actor = e.actor
        .map(gh_user_to_mr)
        .unwrap_or_else(|| MrUser {
            login: "ghost".into(),
            display_name: "Unknown".into(),
            avatar_url: None,
        });

    let (kind, summary) = match e.event.as_str() {
        "labeled"    => ("label", format!("added the **{}** label",
                            e.label.as_ref().map(|l| l.name.as_str()).unwrap_or("?"))),
        "unlabeled"  => ("label", format!("removed the **{}** label",
                            e.label.as_ref().map(|l| l.name.as_str()).unwrap_or("?"))),
        "assigned"   => ("assign", format!("assigned **{}**",
                            e.assignee.as_ref().map(|u| u.login.as_str()).unwrap_or("someone"))),
        "unassigned" => ("assign", format!("unassigned **{}**",
                            e.assignee.as_ref().map(|u| u.login.as_str()).unwrap_or("someone"))),
        "review_requested"       => ("review", format!("requested a review from **{}**",
                            e.requested_reviewer.as_ref().map(|u| u.login.as_str()).unwrap_or("someone"))),
        "review_request_removed" => ("review", format!("removed review request from **{}**",
                            e.requested_reviewer.as_ref().map(|u| u.login.as_str()).unwrap_or("someone"))),
        "closed"     => ("state",  "closed this".to_string()),
        "reopened"   => ("state",  "reopened this".to_string()),
        "merged"     => ("state",  "merged this".to_string()),
        "convert_to_draft"  => ("state", "marked this as a draft".to_string()),
        "ready_for_review"  => ("state", "marked this as ready for review".to_string()),
        "auto_merge_enabled"  => ("state", "enabled auto-merge".to_string()),
        "auto_merge_disabled" => ("state", "disabled auto-merge".to_string()),
        "head_ref_force_pushed" => ("commit", "force-pushed the source branch".to_string()),
        "head_ref_deleted"      => ("commit", "deleted the source branch".to_string()),
        "head_ref_restored"     => ("commit", "restored the source branch".to_string()),
        "renamed"    => {
            let r = e.rename.as_ref();
            let from = r.map(|x| x.from.as_str()).unwrap_or("?");
            let to   = r.map(|x| x.to.as_str()).unwrap_or("?");
            ("rename", format!("renamed from “{from}” to “{to}”"))
        }
        "milestoned"   => ("system", "added a milestone".to_string()),
        "demilestoned" => ("system", "removed a milestone".to_string()),
        "locked"       => ("system", "locked the conversation".to_string()),
        "unlocked"     => ("system", "unlocked the conversation".to_string()),
        // Drop noisy / non-actionable events outright.
        "subscribed" | "unsubscribed" | "mentioned" | "referenced"
            | "head_ref_cleaned" | "marked_as_duplicate" | "unmarked_as_duplicate"
            => return None,
        _ => return None,
    };

    Some(MrEvent {
        id:         e.id.to_string(),
        kind:       kind.to_string(),
        actor,
        summary,
        created_at: e.created_at,
    })
}

/// GitHub bot accounts always carry the canonical `[bot]` suffix on the login
/// (e.g. `dependabot[bot]`, `renovate[bot]`). Conservative — won't catch
/// service accounts that opted out of the suffix.
fn is_bot_login(login: &str) -> bool {
    let l = login.to_ascii_lowercase();
    l.ends_with("[bot]") || l == "github-actions"
}

/// GitLab doesn't expose a flag, so fall back to a name/login heuristic.
/// Matches "...-bot", "...bot", "GitLab Security Bot", etc.
fn is_bot_user_gl(login: &str, display_name: &str) -> bool {
    let l = login.to_ascii_lowercase();
    let n = display_name.to_ascii_lowercase();
    l.ends_with("-bot") || l.ends_with("_bot") || l.contains("bot-")
        || n.contains(" bot") || n.ends_with(" bot") || n.starts_with("bot ")
        || l == "ghost" // ex-deleted user — not a bot per se but not a real human either
}

// ---------------------------------------------------------------------------
// Internal GitLab raw types
// ---------------------------------------------------------------------------

/// GitLab often returns `null` for boolean fields on older MRs.
/// This deserializer treats both absent and null as `false`.
fn bool_or_null<'de, D: serde::Deserializer<'de>>(de: D) -> std::result::Result<bool, D::Error> {
    Option::<bool>::deserialize(de).map(|opt| opt.unwrap_or(false))
}

#[derive(Deserialize)]
struct GlMr {
    iid:            u64,
    title:          String,
    #[serde(default)]
    description:    Option<String>,
    state:          String,
    author:         GlUser,
    source_branch:  String,
    target_branch:  String,
    web_url:        String,
    created_at:     String,
    updated_at:     String,
    #[serde(default)]
    labels:         Vec<String>,
    #[serde(default)]
    assignees:      Vec<GlUser>,
    #[serde(default)]
    reviewers:      Vec<GlUser>,
    #[serde(default)]
    user_notes_count: u32,
    #[serde(default)]
    merge_status:   Option<String>,
    #[serde(default, deserialize_with = "bool_or_null")]
    work_in_progress: bool,
    #[serde(default, deserialize_with = "bool_or_null")]
    draft: bool,
    #[serde(default, deserialize_with = "bool_or_null")]
    squash: bool,
    #[serde(default, deserialize_with = "bool_or_null")]
    should_remove_source_branch: bool,
    #[serde(default, deserialize_with = "bool_or_null")]
    force_remove_source_branch: bool,
    /// GitLab "Merge When Pipeline Succeeds" — set when the MR is armed for
    /// auto-merge. The newer alias `merge_when_checks_pass` (GitLab 16+) maps
    /// to the same field on the API response.
    #[serde(default, deserialize_with = "bool_or_null")]
    merge_when_pipeline_succeeds: bool,
    #[serde(default)]
    merge_commit_sha:  Option<String>,
    /// Present only for squash merges; takes precedence over merge_commit_sha.
    #[serde(default)]
    squash_commit_sha: Option<String>,
    /// SHA of the source branch HEAD at last update (GitLab's top-level "sha" field).
    #[serde(default)]
    sha: Option<String>,
}

#[derive(Deserialize)]
struct GlUser {
    username:   String,
    name:       String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GlNote {
    id:         i64,
    author:     GlUser,
    body:       String,
    created_at: String,
    system:     bool,
}

fn gitlab_mr_to_mr(m: GlMr) -> MergeRequest {
    let state = match m.state.as_str() {
        "merged" => MrState::Merged,
        "closed" => MrState::Closed,
        _        => MrState::Open,
    };
    let mergeable = match m.merge_status.as_deref() {
        Some("can_be_merged") => Some(true),
        Some("cannot_be_merged") | Some("cannot_be_merged_recheck") => Some(false),
        _ => None,
    };
    let is_draft = m.draft || m.work_in_progress;
    MergeRequest {
        number:        m.iid,
        title:         m.title,
        description:   m.description.unwrap_or_default(),
        state,
        is_draft,
        author:        gl_user_to_mr(m.author),
        source_branch: m.source_branch,
        target_branch: m.target_branch,
        web_url:       m.web_url,
        created_at:    m.created_at,
        updated_at:    m.updated_at,
        labels:        m.labels.into_iter().map(|l| MrLabel { name: l.clone(), color: "6f7178".into() }).collect(),
        assignees:     m.assignees.into_iter().map(gl_user_to_mr).collect(),
        reviewers:     m.reviewers.into_iter().map(gl_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable,
        provider:      "gitlab".into(),
        comments_count: m.user_notes_count,
        squash:        m.squash,
        delete_branch: m.should_remove_source_branch || m.force_remove_source_branch,
        // For squash merges prefer squash_commit_sha; fall back to merge_commit_sha.
        merge_commit_sha: m.squash_commit_sha.or(m.merge_commit_sha),
        head_sha:         m.sha.unwrap_or_default(),
        base_sha:         String::new(), // GitLab API doesn't expose base SHA in list response
        auto_merge_enabled: m.merge_when_pipeline_succeeds,
    }
}

fn gl_user_to_mr(u: GlUser) -> MrUser {
    MrUser {
        login:        u.username,
        display_name: u.name,
        avatar_url:   u.avatar_url,
    }
}

fn gl_note_to_comment(n: GlNote) -> MrComment {
    let is_bot = is_bot_user_gl(&n.author.username, &n.author.name);
    MrComment {
        id:         n.id.to_string(),
        author:     gl_user_to_mr(n.author),
        body:       n.body,
        created_at: n.created_at,
        is_bot,
    }
}

/// Convert a GitLab system note into a timeline event. The body itself is
/// pre-rendered ("approved this merge request", "added 1 commit", "marked
/// this merge request as draft", …), so we just attach it as the summary
/// and pick a coarse `kind` heuristically for icon/grouping purposes.
fn gl_system_note_to_event(n: GlNote) -> MrEvent {
    let lower = n.body.to_ascii_lowercase();
    let kind  = if      lower.contains("approved")                   { "review"  }
                else if lower.contains("unapproved")                 { "review"  }
                else if lower.contains("assigned")                   { "assign"  }
                else if lower.contains("requested review")
                     || lower.contains("review requested")           { "review"  }
                else if lower.contains("added") && lower.contains("label")
                     || lower.contains("removed") && lower.contains("label")
                     || lower.contains("scoped label")               { "label"   }
                else if lower.contains("added")  && lower.contains("commit")
                     || lower.contains("force-pushed")
                     || lower.contains("pushed ")                    { "commit"  }
                else if lower.contains("merged") || lower.contains("closed")
                     || lower.contains("reopened")
                     || lower.contains("marked")  && lower.contains("draft")
                     || lower.contains("marked")  && lower.contains("ready")  { "state"  }
                else if lower.contains("changed title")
                     || lower.contains("changed description")        { "rename"  }
                else { "system" };
    MrEvent {
        id:         n.id.to_string(),
        kind:       kind.to_string(),
        actor:      gl_user_to_mr(n.author),
        summary:    n.body,
        created_at: n.created_at,
    }
}

// ---------------------------------------------------------------------------
// PR / MR — File diffs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhPrFile {
    filename:  String,
    status:    String,
    additions: u32,
    deletions: u32,
    #[serde(default)]
    patch:     Option<String>,
}

#[derive(Deserialize)]
struct GlMrDiff {
    new_path:     String,
    new_file:     bool,
    deleted_file: bool,
    renamed_file: bool,
    diff:         String,
}

pub async fn get_github_pr_files(
    owner:  &str,
    repo:   &str,
    number: u64,
    token:  &str,
) -> Result<Vec<MrFileDiff>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{number}/files?per_page=100"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub PR files request failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub PR files {s}: {b}")));
    }
    let files: Vec<GhPrFile> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub PR files parse: {e}")))?;
    Ok(files.into_iter().map(|f| MrFileDiff {
        filename:  f.filename,
        status:    f.status,
        additions: f.additions,
        deletions: f.deletions,
        patch:     f.patch,
    }).collect())
}

pub async fn get_gitlab_mr_files(
    path:   &str,
    base:   &str,
    number: u64,
    token:  &str,
) -> Result<Vec<MrFileDiff>> {
    let encoded = percent_encode_slash(path);
    let url = format!("{base}/api/v4/projects/{encoded}/merge_requests/{number}/diffs?per_page=100");
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab MR diffs {s}: {b}")));
    }
    let diffs: Vec<GlMrDiff> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab MR diffs parse: {e}")))?;
    Ok(diffs.into_iter().map(|d| {
        let status = if d.new_file          { "added".into() }
                     else if d.deleted_file  { "removed".into() }
                     else if d.renamed_file  { "renamed".into() }
                     else                    { "modified".into() };
        let additions = d.diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count() as u32;
        let deletions = d.diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count() as u32;
        MrFileDiff {
            filename: d.new_path,
            status,
            additions,
            deletions,
            patch: if d.diff.is_empty() { None } else { Some(d.diff) },
        }
    }).collect())
}

// ---------------------------------------------------------------------------
// PR / MR — Commits
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhPrCommit {
    sha:      String,
    commit:   GhCommitInner,
    html_url: String,
}

#[derive(Deserialize)]
struct GhCommitInner {
    message: String,
    author:  GhCommitAuthor,
}

#[derive(Deserialize)]
struct GhCommitAuthor {
    name: String,
    date: String,
}

#[derive(Deserialize)]
struct GlMrCommitItem {
    id:          String,
    title:       String,
    author_name: String,
    created_at:  String,
    #[serde(default)]
    web_url:     Option<String>,
}

pub async fn get_github_pr_commits(
    owner:  &str,
    repo:   &str,
    number: u64,
    token:  &str,
) -> Result<Vec<MrCommit>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{number}/commits?per_page=100"
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub PR commits request failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub PR commits {s}: {b}")));
    }
    let commits: Vec<GhPrCommit> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub PR commits parse: {e}")))?;
    Ok(commits.into_iter().map(|c| MrCommit {
        sha:     c.sha.clone(),
        message: c.commit.message.lines().next().unwrap_or("").to_string(),
        author:  c.commit.author.name,
        date:    c.commit.author.date,
        web_url: Some(c.html_url),
    }).collect())
}

pub async fn get_gitlab_mr_commits(
    path:   &str,
    base:   &str,
    number: u64,
    token:  &str,
) -> Result<Vec<MrCommit>> {
    let encoded = percent_encode_slash(path);
    let url = format!("{base}/api/v4/projects/{encoded}/merge_requests/{number}/commits?per_page=100");
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab MR commits {s}: {b}")));
    }
    let commits: Vec<GlMrCommitItem> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab MR commits parse: {e}")))?;
    Ok(commits.into_iter().map(|c| MrCommit {
        sha:     c.id.clone(),
        message: c.title,
        author:  c.author_name,
        date:    c.created_at,
        web_url: c.web_url,
    }).collect())
}

// ---------------------------------------------------------------------------
// Single-commit file diff (for Commits tab drill-down)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhCommitResponse {
    #[serde(default)]
    files: Vec<GhPrFile>,
}

pub async fn get_github_commit_files(
    owner: &str,
    repo:  &str,
    sha:   &str,
    token: &str,
) -> Result<Vec<MrFileDiff>> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "arbor-git-gui/1.0")
        .send().await
        .map_err(|e| AppError::Other(format!("GitHub commit request failed: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitHub commit {s}: {b}")));
    }
    let commit: GhCommitResponse = resp.json().await
        .map_err(|e| AppError::Other(format!("GitHub commit parse: {e}")))?;
    Ok(commit.files.into_iter().map(|f| MrFileDiff {
        filename:  f.filename,
        status:    f.status,
        additions: f.additions,
        deletions: f.deletions,
        patch:     f.patch,
    }).collect())
}

pub async fn get_gitlab_commit_files(
    path:  &str,
    base:  &str,
    sha:   &str,
    token: &str,
) -> Result<Vec<MrFileDiff>> {
    let encoded = percent_encode_slash(path);
    let url = format!("{base}/api/v4/projects/{encoded}/repository/commits/{sha}/diff");
    let client = reqwest::Client::new();
    let resp = crate::git_provider::ci_impl::gitlab_send_with_refresh(
        |tok| client.get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", "arbor-git-gui/1.0"),
        base,
        token,
    ).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::Other(format!("GitLab commit diff {s}: {b}")));
    }
    let diffs: Vec<GlMrDiff> = resp.json().await
        .map_err(|e| AppError::Other(format!("GitLab commit diff parse: {e}")))?;
    Ok(diffs.into_iter().map(|d| {
        let status    = if d.new_file          { "added".into() }
                        else if d.deleted_file { "removed".into() }
                        else if d.renamed_file { "renamed".into() }
                        else                   { "modified".into() };
        let additions = d.diff.lines().filter(|l| l.starts_with('+') && !l.starts_with("+++")).count() as u32;
        let deletions = d.diff.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count() as u32;
        MrFileDiff { filename: d.new_path, status, additions, deletions,
                     patch: if d.diff.is_empty() { None } else { Some(d.diff) } }
    }).collect())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn percent_encode_slash(s: &str) -> String {
    s.replace('/', "%2F")
}

// Re-export token helpers for use in commands.
pub use crate::git_provider::ci_impl::{get_github_token, get_gitlab_token};
