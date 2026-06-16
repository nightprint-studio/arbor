use serde::Deserialize;
use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::{detect_from_remotes, CiProviderInfo};

// ---------------------------------------------------------------------------
// Public types — defined in `corvus-git-provider-api`, re-exported here so the
// REST client code below and external `mr_impl::*` call sites keep resolving.
// ---------------------------------------------------------------------------

pub use corvus_git_provider_api::mr::*;

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
    let url    = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
    // Route through the refresh wrapper so an expired OAuth access token gets
    // rotated and the create retried, matching every other GitHub call. Before
    // this, opening a PR after the token expired surfaced a raw 401 to the
    // user and forced a manual reconnect from Settings.
    let resp = crate::git_provider::ci_impl::github_send_with_refresh(
        |tok| client
            .post(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(&body),
        token,
    ).await?;

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
        return Err(map_auto_merge_error(&b)
            .unwrap_or_else(|| AppError::Other(format!("GitHub auto-merge {s}: {b}"))));
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
        return Err(map_auto_merge_error(&msg).unwrap_or_else(|| AppError::Other(msg)));
    }
    Ok(())
}

/// Detect the well-known "PR/MR is not mergeable" failure modes from a raw
/// provider error message and re-phrase them so the user understands *why*
/// auto-merge couldn't be armed. Returns `None` when the message doesn't match
/// any recognised shape — the caller falls back to surfacing the raw response.
///
/// GitHub variants (GraphQL + REST):
///   - "Pull request is in clean status"  → no protection, no checks pending
///   - "Pull request is in unstable status" / "is in dirty status" → conflicts
///   - "Pull request Pull Request is not in the correct state"
/// GitLab variants (REST `merge` endpoint response body):
///   - "Branch cannot be merged"
///   - "merge request is not mergeable"
///   - JSON {"message":"406 Branch cannot be merged"} / {"message":"...conflict..."}
fn map_auto_merge_error(raw: &str) -> Option<AppError> {
    // Try JSON first — GitLab + GitHub REST both wrap the message.
    let probe: String = if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        v.get("message")
            .and_then(|m| m.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };
    let lower = probe.to_lowercase();

    if lower.contains("dirty") || lower.contains("conflict")
        || lower.contains("cannot be merged") || lower.contains("not mergeable")
    {
        return Some(AppError::Other(
            "This pull request has conflicts that must be resolved before \
             auto-merge can be enabled. Rebase or merge the target branch in, \
             fix the conflicts, then push.".into()
        ));
    }
    if lower.contains("clean status") {
        return Some(AppError::Other(
            "Auto-merge needs a pending check or required review to gate on. \
             This pull request is already mergeable — merge it directly instead.".into()
        ));
    }
    if lower.contains("auto_merge") && lower.contains("disabled") {
        return Some(AppError::Other(
            "Auto-merge is disabled for this repository. Enable it in the \
             repository settings, then try again.".into()
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// GitLab Merge Requests
// ---------------------------------------------------------------------------

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
    Err(map_auto_merge_error(&b)
        .unwrap_or_else(|| AppError::Other(format!("GitLab auto-merge {s}: {b}"))))
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

fn gh_user_to_mr(u: GhUser) -> MrUser {
    MrUser {
        login:        u.login.clone(),
        display_name: u.name.unwrap_or(u.login),
        avatar_url:   u.avatar_url,
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
