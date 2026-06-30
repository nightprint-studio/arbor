//! GitHub MR (Pull Request) operations.
//!
//! Folds the old `git_provider::github::mr` delegate (RepoRef/MrId
//! destructuring, `map_state`, the "merged"-filter retain, and the
//! `add_mr_comment` placeholder) together with the REST bodies that lived in
//! `git_provider::mr_impl` (`list_github_prs`, `get_github_pr`, …). Behavior is
//! preserved byte-for-byte: every error string, header, URL, query param, HTTP
//! method and status check is identical to the originals — only the auth/refresh
//! plumbing now goes through [`GithubHttp`] and `AppError::Other(msg)` became
//! `classify(msg)`.

use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::{classify, GithubHttp};

// ---------------------------------------------------------------------------
// Delegate helpers (destructuring / validation)
// ---------------------------------------------------------------------------

fn repo_parts(repo: &RepoRef) -> Result<(&str, &str), ProviderError> {
    let owner = repo.owner_or_path.as_str();
    let name = repo.name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub RepoRef requires name".into())
    })?;
    Ok((owner, name))
}

fn id_parts(id: &MrId) -> Result<(&str, &str, u64), ProviderError> {
    if !matches!(id.provider, ProviderKind::GitHub) {
        return Err(ProviderError::BadRequest(
            "MrId provider mismatch (expected GitHub)".into(),
        ));
    }
    let owner = id.owner_or_path.as_str();
    let name = id.repo_name.as_deref().ok_or_else(|| {
        ProviderError::BadRequest("GitHub MrId requires repo_name".into())
    })?;
    Ok((owner, name, id.number))
}

fn map_state(s: Option<&str>) -> &'static str {
    match s {
        Some("closed") => "closed",
        Some("merged") => "closed", // GH treats merged as closed; filter client-side
        Some("all") => "all",
        _ => "open",
    }
}

// ---------------------------------------------------------------------------
// Trait-surface free functions (delegate behavior, REST inlined)
// ---------------------------------------------------------------------------

pub(crate) async fn list_mrs(
    http: &GithubHttp,
    repo: &RepoRef,
    filter: MrFilter,
) -> Result<Vec<MrInfo>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let api_state = map_state(filter.state.as_deref());
    let mut prs = list_github_prs(http, owner, name, api_state).await?;
    // GitHub has no native "merged" filter — it returns merged as "closed".
    // When the caller asked for "merged" specifically, drop closed-not-merged.
    if matches!(filter.state.as_deref(), Some("merged")) {
        prs.retain(|p| matches!(p.state, MrState::Merged));
    }
    Ok(prs)
}

pub(crate) async fn get_mr(http: &GithubHttp, id: &MrId) -> Result<MrDetail, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    get_github_pr(http, owner, name, number).await
}

pub(crate) async fn create_mr(
    http: &GithubHttp,
    repo: &RepoRef,
    req: MrCreateRequest,
) -> Result<MrInfo, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let (mr, _node_id) = create_github_pr(http, owner, name, &req).await?;
    Ok(mr)
}

pub(crate) async fn update_mr(
    _http: &GithubHttp,
    _id: &MrId,
    _req: MrUpdateRequest,
) -> Result<MrInfo, ProviderError> {
    Err(ProviderError::Unsupported { feature: "update_mr".into() })
}

pub(crate) async fn close_mr(http: &GithubHttp, id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    update_github_pr_state(http, owner, name, number, "closed").await
}

pub(crate) async fn reopen_mr(http: &GithubHttp, id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    update_github_pr_state(http, owner, name, number, "open").await
}

pub(crate) async fn merge_mr(
    http: &GithubHttp,
    id: &MrId,
    opts: MergeOpts,
) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let strategy = opts
        .strategy
        .as_deref()
        .map(|s| s.to_lowercase())
        .filter(|s| matches!(s.as_str(), "merge" | "squash" | "rebase"))
        .unwrap_or_else(|| if opts.squash { "squash".into() } else { "merge".into() });
    merge_github_pr(http, owner, name, number, &strategy).await
}

pub(crate) async fn list_mr_comments(
    _http: &GithubHttp,
    _id: &MrId,
) -> Result<Vec<MrComment>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_comments (use get_mr)".into() })
}

pub(crate) async fn add_mr_comment(
    http: &GithubHttp,
    id: &MrId,
    body: &str,
) -> Result<MrComment, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    add_github_pr_comment(http, owner, name, number, body).await?;
    // GitHub's add-comment endpoint returns the created comment, but the
    // existing helper discards it — Phase 5 will fix this. For now return a
    // minimal placeholder so the trait surface is honored.
    Ok(MrComment {
        id: "0".into(),
        author: MrUser {
            login: "".into(),
            display_name: "".into(),
            avatar_url: None,
        },
        body: body.into(),
        created_at: String::new(),
        is_bot: false,
    })
}

pub(crate) async fn list_mr_files(
    http: &GithubHttp,
    id: &MrId,
) -> Result<Vec<MrFile>, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    get_github_pr_files(http, owner, name, number).await
}

pub(crate) async fn fetch_mr_diff(
    _http: &GithubHttp,
    _id: &MrId,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_mr_diff (use list_mr_files)".into() })
}

pub(crate) async fn check_mr_conflict(
    _http: &GithubHttp,
    _id: &MrId,
) -> Result<MrConflict, ProviderError> {
    Err(ProviderError::Unsupported { feature: "check_mr_conflict".into() })
}

pub(crate) async fn list_mr_reviewers(
    _http: &GithubHttp,
    _id: &MrId,
) -> Result<Vec<ProviderUser>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_reviewers".into() })
}

pub(crate) async fn request_mr_review(
    _http: &GithubHttp,
    _id: &MrId,
    _user: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "request_mr_review".into() })
}

pub(crate) async fn approve_mr(
    _http: &GithubHttp,
    _id: &MrId,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "approve_mr".into() })
}

// ---------------------------------------------------------------------------
// REST functions (ported verbatim from mr_impl, via GithubHttp)
// ---------------------------------------------------------------------------

pub(crate) async fn list_github_prs(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    state: &str, // "open" | "closed" | "all"
) -> Result<Vec<MergeRequest>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls?state={state}&per_page=50&sort=updated&direction=desc"
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub API {status}: {body}")));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| classify(format!("GitHub PR body read error: {e}")))?;
    let prs: Vec<GhPr> = serde_json::from_str(&body).map_err(|e| {
        classify(format!(
            "GitHub PR parse error: {e} — body: {}",
            &body[..body.len().min(300)]
        ))
    })?;

    Ok(prs.into_iter().map(|p| github_pr_to_mr(p, owner, repo)).collect())
}

pub(crate) async fn get_github_pr(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<MrDetail, ProviderError> {
    // Fetch PR itself
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let pr_resp = http
        .send(|s| {
            http.client()
                .get(&pr_url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !pr_resp.status().is_success() {
        let s = pr_resp.status();
        let b = pr_resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub API {s}: {b}")));
    }
    let pr: GhPrDetail = pr_resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub PR detail parse: {e}")))?;

    let mut mr = github_pr_detail_to_mr(pr, owner, repo);

    // Fetch issue comments (general comments on the PR thread)
    let comments_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments?per_page=50"
    );
    let comments_resp = http
        .send(|s| {
            http.client()
                .get(&comments_url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await;

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
    let events_resp = http
        .send(|s| {
            http.client()
                .get(&events_url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await;

    let events: Vec<MrEvent> = match events_resp {
        Ok(r) if r.status().is_success() => {
            let raw: Vec<GhEvent> = r.json().await.unwrap_or_default();
            raw.into_iter().filter_map(gh_event_to_mr).collect()
        }
        _ => vec![],
    };
    mr.events = events;

    // Checks left empty; checks_status on the MR itself shows the summary.
    mr.checks = vec![];
    Ok(mr)
}

/// Returns `(MergeRequest, Option<node_id>)`.  `node_id` is the GraphQL Relay
/// ID of the PR — required to enable auto-merge via the GraphQL mutation.
pub(crate) async fn create_github_pr(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    params: &CreateMrParams,
) -> Result<(MergeRequest, Option<String>), ProviderError> {
    let body = serde_json::json!({
        "title": params.title,
        "body":  params.description.as_deref().unwrap_or(""),
        "head":  params.source_branch,
        "base":  params.target_branch,
        "draft": params.is_draft,
    });
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls");
    // Route through the refresh wrapper so an expired OAuth access token gets
    // rotated and the create retried, matching every other GitHub call. Before
    // this, opening a PR after the token expired surfaced a raw 401 to the
    // user and forced a manual reconnect from Settings.
    let resp = http
        .send(|s| {
            http.client()
                .post(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(github_create_pr_error(s, b, params));
    }
    let raw: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub create PR parse: {e}")))?;
    let node_id = raw.get("node_id").and_then(|v| v.as_str()).map(ToOwned::to_owned);
    let pr: GhPr = serde_json::from_value(raw)
        .map_err(|e| classify(format!("GitHub create PR parse: {e}")))?;
    let mut mr = github_pr_to_mr(pr, owner, repo);
    mr.squash = params.squash;
    mr.delete_branch = params.delete_branch;
    Ok((mr, node_id))
}

/// Map a GitHub "create PR" failure to a human-readable message.  GitHub
/// 422s are particularly cryptic ("head: invalid") without context — we
/// translate the common shapes and fall back to the raw body only when we
/// don't recognise the error.
fn github_create_pr_error(
    status: reqwest::StatusCode,
    body: String,
    params: &CreateMrParams,
) -> ProviderError {
    let parsed: std::result::Result<serde_json::Value, _> = serde_json::from_str(&body);
    if let Ok(v) = parsed {
        let message = v.get("message").and_then(|m| m.as_str()).unwrap_or("");
        if let Some(errs) = v.get("errors").and_then(|e| e.as_array()) {
            for e in errs {
                let field = e.get("field").and_then(|f| f.as_str()).unwrap_or("");
                let code = e.get("code").and_then(|c| c.as_str()).unwrap_or("");
                match (field, code) {
                    ("head", "invalid") => {
                        return classify(format!(
                            "Source branch '{}' was not found on the GitHub remote. \
                         Push it to origin first, then try again.",
                            params.source_branch,
                        ))
                    }
                    ("base", "invalid") => {
                        return classify(format!(
                            "Target branch '{}' does not exist on the GitHub remote.",
                            params.target_branch,
                        ))
                    }
                    (_, "missing_field") => {
                        return classify(format!(
                            "GitHub rejected the PR: required field '{field}' is missing.",
                        ))
                    }
                    (_, "custom") => {
                        if let Some(m) = e.get("message").and_then(|m| m.as_str()) {
                            return classify(format!("GitHub: {m}"));
                        }
                    }
                    _ => {}
                }
            }
        }
        let lower = message.to_lowercase();
        if lower.contains("no commits between") {
            return classify(format!(
                "No commits between '{}' and '{}' — nothing to merge yet. \
                 Commit and push your changes first.",
                params.target_branch, params.source_branch,
            ));
        }
        if lower.contains("a pull request already exists") {
            return classify(format!(
                "A pull request already exists for '{}' → '{}'.",
                params.source_branch, params.target_branch,
            ));
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return classify(
                "GitHub refused the request — check your token permissions in \
                 Settings → Git & Integrations."
                    .into(),
            );
        }
        if !message.is_empty() {
            return classify(format!("GitHub: {message}"));
        }
    }
    // Unknown shape — surface status + trimmed body so the issue can still be diagnosed.
    let trimmed: String = body.chars().take(300).collect();
    classify(format!("GitHub create PR {status}: {trimmed}"))
}

pub(crate) async fn merge_github_pr(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
    merge_method: &str, // "merge" | "squash" | "rebase"
) -> Result<(), ProviderError> {
    let body = serde_json::json!({ "merge_method": merge_method });
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}/merge");
    let resp = http
        .send(|s| {
            http.client()
                .put(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() || resp.status().as_u16() == 200 {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub merge PR {s}: {b}")))
}

pub(crate) async fn update_github_pr_state(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
    state: &str, // "open" | "closed"
) -> Result<(), ProviderError> {
    let body = serde_json::json!({ "state": state });
    let url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let resp = http
        .send(|s| {
            http.client()
                .patch(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub update PR {s}: {b}")))
}

pub(crate) async fn add_github_pr_comment(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
    body: &str,
) -> Result<(), ProviderError> {
    let payload = serde_json::json!({ "body": body });
    let url = format!("https://api.github.com/repos/{owner}/{repo}/issues/{number}/comments");
    let resp = http
        .send(|s| {
            http.client()
                .post(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&payload)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitHub add comment {s}: {b}")))
}

pub(crate) async fn get_github_pr_files(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{number}/files?per_page=100"
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub PR files {s}: {b}")));
    }
    let files: Vec<GhPrFile> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub PR files parse: {e}")))?;
    Ok(files
        .into_iter()
        .map(|f| MrFileDiff {
            filename: f.filename,
            status: f.status,
            additions: f.additions,
            deletions: f.deletions,
            patch: f.patch,
        })
        .collect())
}

/// Query the GitHub repo endpoint and return the value of `allow_auto_merge`.
/// Falls back to `true` when the field is absent (e.g. unauthenticated /
/// non-admin response) so the option stays available — the create call will
/// surface a notification if it ultimately fails.
pub(crate) async fn fetch_github_auto_merge_allowed(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
) -> Result<bool, ProviderError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub repo fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub repo parse: {e}")))?;
    // Treat a missing field as "unknown → allow"; only an explicit false disables.
    Ok(v.get("allow_auto_merge").and_then(|b| b.as_bool()).unwrap_or(true))
}

/// GitHub probe: archived or disabled repos cannot accept new PRs and the
/// `/pulls` endpoint may 404 on certain configurations.
///
/// TODO: GitHub has no granular `has_pull_requests` flag. Edge cases we
/// don't yet catch: fork-mirrors whose upstream blocks PRs, repos with
/// branch-protection forbidding PRs entirely. If 404 keeps surfacing in
/// the wild, add a `list_mrs(per_page=1)` fallback here.
pub(crate) async fn fetch_github_pr_feature_enabled(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
) -> Result<MrFeatureStatus, ProviderError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub repo fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub repo parse: {e}")))?;
    let archived = v.get("archived").and_then(|b| b.as_bool()).unwrap_or(false);
    let disabled = v.get("disabled").and_then(|b| b.as_bool()).unwrap_or(false);
    if disabled {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason: Some("This repository is disabled on GitHub.".into()),
        });
    }
    if archived {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason: Some(
                "This repository is archived — new pull requests cannot be opened.".into(),
            ),
        });
    }
    Ok(MrFeatureStatus::default())
}

pub(crate) async fn list_mr_commits(
    http: &GithubHttp,
    id: &MrId,
) -> Result<Vec<MrCommit>, ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    get_github_pr_commits(http, owner, name, number).await
}

pub(crate) async fn get_commit_diff(
    http: &GithubHttp,
    repo: &RepoRef,
    sha: &str,
) -> Result<Vec<MrFile>, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    get_github_commit_files(http, owner, name, sha).await
}

pub(crate) async fn mark_mr_ready(http: &GithubHttp, id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    mark_github_pr_ready(http, owner, name, number).await
}

pub(crate) async fn enable_auto_merge(
    http: &GithubHttp,
    id: &MrId,
    opts: AutoMergeOpts,
) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    let node_id = fetch_github_pr_node_id(http, owner, name, number).await?;
    let merge_method = if opts.squash { "SQUASH" } else { "MERGE" };
    enable_github_auto_merge(http, &node_id, merge_method).await
}

pub(crate) async fn disable_auto_merge(http: &GithubHttp, id: &MrId) -> Result<(), ProviderError> {
    let (owner, name, number) = id_parts(id)?;
    disable_github_auto_merge(http, owner, name, number).await
}

pub(crate) async fn auto_merge_allowed(
    http: &GithubHttp,
    repo: &RepoRef,
) -> Result<MrCapabilities, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    let allowed = fetch_github_auto_merge_allowed(http, owner, name).await?;
    if allowed {
        Ok(MrCapabilities::default())
    } else {
        Ok(MrCapabilities {
            auto_merge_supported: false,
            auto_merge_reason: Some(
                "Auto-merge is disabled for this repository — \
                 enable it under Settings → General → Pull Requests on GitHub."
                    .into(),
            ),
        })
    }
}

pub(crate) async fn mr_feature_status(
    http: &GithubHttp,
    repo: &RepoRef,
) -> Result<MrFeatureStatus, ProviderError> {
    let (owner, name) = repo_parts(repo)?;
    fetch_github_pr_feature_enabled(http, owner, name).await
}

// ---------------------------------------------------------------------------
// PR commits + single-commit diff
// ---------------------------------------------------------------------------

pub(crate) async fn get_github_pr_commits(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<Vec<MrCommit>, ProviderError> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls/{number}/commits?per_page=100"
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub PR commits {s}: {b}")));
    }
    let commits: Vec<GhPrCommit> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub PR commits parse: {e}")))?;
    Ok(commits
        .into_iter()
        .map(|c| MrCommit {
            sha: c.sha.clone(),
            message: c.commit.message.lines().next().unwrap_or("").to_string(),
            author: c.commit.author.name,
            date: c.commit.author.date,
            web_url: Some(c.html_url),
        })
        .collect())
}

pub(crate) async fn get_github_commit_files(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    sha: &str,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub commit {s}: {b}")));
    }
    let commit: GhCommitResponse = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub commit parse: {e}")))?;
    Ok(commit
        .files
        .into_iter()
        .map(|f| MrFileDiff {
            filename: f.filename,
            status: f.status,
            additions: f.additions,
            deletions: f.deletions,
            patch: f.patch,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Mark ready / auto-merge (REST node_id lookup + GraphQL mutations)
// ---------------------------------------------------------------------------

/// Resolve the GraphQL Relay `node_id` of a PR via REST `/pulls/{n}`. Shared by
/// mark-ready, enable- and disable-auto-merge (GraphQL needs the node id, which
/// the REST PR payload carries).
async fn fetch_github_pr_node_id(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<String, ProviderError> {
    let pr_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
    let resp = http
        .send(|s| {
            http.client()
                .get(&pr_url)
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub PR lookup {s}: {b}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub PR parse: {e}")))?;
    v.get("node_id")
        .and_then(|n| n.as_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| classify("GitHub PR response missing node_id".into()))
}

/// POST a GraphQL document through the session/refresh wrapper. The shell used
/// a raw `Bearer {token}` header; here the session's `auth_header` (already
/// `Bearer …` for OAuth) is reused so the 401→refresh→retry path applies.
async fn github_graphql(
    http: &GithubHttp,
    body: &serde_json::Value,
) -> Result<reqwest::Response, ProviderError> {
    http.send(|s| {
        http.client()
            .post("https://api.github.com/graphql")
            .header("Authorization", &s.auth_header)
            .header("Content-Type", "application/json")
            .header("User-Agent", "arbor-git-gui/1.0")
            .json(body)
    })
    .await
}

/// Mark a GitHub PR as ready for review (removes draft status).
/// GitHub's REST API does NOT support converting draft→ready; requires GraphQL.
pub(crate) async fn mark_github_pr_ready(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), ProviderError> {
    // Step 1: fetch the PR node_id (required by GraphQL).
    let node_id = fetch_github_pr_node_id(http, owner, repo, number)
        .await
        .map_err(|e| classify(format!("GitHub mark ready (fetch node_id) {e}")))?;

    // Step 2: GraphQL mutation markPullRequestReadyForReview.
    let query = "mutation MarkReady($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { pullRequest { isDraft } } }";
    let gql_body = serde_json::json!({ "query": query, "variables": { "id": node_id } });

    let gql_resp = github_graphql(http, &gql_body)
        .await
        .map_err(|e| classify(format!("GitHub mark ready (GraphQL) {e}")))?;

    if !gql_resp.status().is_success() {
        let s = gql_resp.status();
        let b = gql_resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub mark ready (GraphQL) {s}: {b}")));
    }

    // GraphQL always returns 200; check for errors in the response body.
    let gql_data: serde_json::Value = gql_resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub mark ready (GraphQL parse): {e}")))?;
    if let Some(errors) = gql_data.get("errors") {
        let msg = errors.to_string();
        return Err(classify(format!("GitHub mark ready (GraphQL errors): {msg}")));
    }

    Ok(())
}

/// Enable auto-merge on a GitHub PR via GraphQL.
/// `merge_method` is one of `"MERGE" | "SQUASH" | "REBASE"`.
pub(crate) async fn enable_github_auto_merge(
    http: &GithubHttp,
    pr_node_id: &str,
    merge_method: &str,
) -> Result<(), ProviderError> {
    let method = match merge_method.to_uppercase().as_str() {
        "SQUASH" => "SQUASH",
        "REBASE" => "REBASE",
        _ => "MERGE",
    };
    let query = "mutation($id: ID!, $m: PullRequestMergeMethod!) { \
        enablePullRequestAutoMerge(input: { pullRequestId: $id, mergeMethod: $m }) { \
            pullRequest { autoMergeRequest { enabledAt } } \
        } }";
    let body =
        serde_json::json!({ "query": query, "variables": { "id": pr_node_id, "m": method } });

    let resp = github_graphql(http, &body).await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(map_auto_merge_error(&b)
            .unwrap_or_else(|| classify(format!("GitHub auto-merge {s}: {b}"))));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub auto-merge parse: {e}")))?;
    if let Some(errs) = data.get("errors") {
        // Extract the first error message for a concise user-facing notice.
        let msg = errs
            .get(0)
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&errs.to_string())
            .to_string();
        return Err(map_auto_merge_error(&msg).unwrap_or_else(|| classify(msg)));
    }
    Ok(())
}

/// Disable auto-merge on a GitHub PR. Looks up the PR's GraphQL node ID first
/// (REST `/pulls/{n}` carries `node_id`), then sends the `disablePullRequestAutoMerge`
/// mutation. The mutation is a no-op when auto-merge isn't currently armed.
pub(crate) async fn disable_github_auto_merge(
    http: &GithubHttp,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), ProviderError> {
    // Resolve node_id via REST so we don't need a second auth flow for GraphQL.
    let node_id = fetch_github_pr_node_id(http, owner, repo, number).await?;

    let query = "mutation($id: ID!) { \
        disablePullRequestAutoMerge(input: { pullRequestId: $id }) { \
            pullRequest { number } \
        } }";
    let body = serde_json::json!({ "query": query, "variables": { "id": node_id } });

    let resp = github_graphql(http, &body)
        .await
        .map_err(|e| classify(format!("GitHub disable auto-merge {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitHub disable auto-merge {s}: {b}")));
    }
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitHub disable auto-merge parse: {e}")))?;
    if let Some(errs) = data.get("errors") {
        let msg = errs
            .get(0)
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or(&errs.to_string())
            .to_string();
        return Err(classify(msg));
    }
    Ok(())
}

/// Detect the well-known "PR is not mergeable" failure modes from a raw provider
/// error message and re-phrase them so the user understands *why* auto-merge
/// couldn't be armed. Returns `None` when the message doesn't match any
/// recognised shape — the caller falls back to surfacing the raw response.
fn map_auto_merge_error(raw: &str) -> Option<ProviderError> {
    // Try JSON first — GitHub REST/GraphQL wraps the message.
    let probe: String = if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        v.get("message")
            .and_then(|m| m.as_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| raw.to_string())
    } else {
        raw.to_string()
    };
    let lower = probe.to_lowercase();

    if lower.contains("dirty")
        || lower.contains("conflict")
        || lower.contains("cannot be merged")
        || lower.contains("not mergeable")
    {
        return Some(classify(
            "This pull request has conflicts that must be resolved before \
             auto-merge can be enabled. Rebase or merge the target branch in, \
             fix the conflicts, then push."
                .into(),
        ));
    }
    if lower.contains("clean status") {
        return Some(classify(
            "Auto-merge needs a pending check or required review to gate on. \
             This pull request is already mergeable — merge it directly instead."
                .into(),
        ));
    }
    if lower.contains("auto_merge") && lower.contains("disabled") {
        return Some(classify(
            "Auto-merge is disabled for this repository. Enable it in the \
             repository settings, then try again."
                .into(),
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// Internal GitHub raw types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GhPr {
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    state: String,
    draft: Option<bool>,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    merge_commit_sha: Option<String>,
    user: GhUser,
    head: GhRef,
    base: GhRef,
    html_url: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    labels: Vec<GhLabel>,
    #[serde(default)]
    assignees: Vec<GhUser>,
    #[serde(default)]
    requested_reviewers: Vec<GhUser>,
    #[serde(default)]
    comments: u32,
    /// GitHub returns an object (`{ enabled_by, merge_method, commit_title, … }`)
    /// when auto-merge is armed, `null` otherwise. We only care about presence.
    #[serde(default)]
    auto_merge: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GhPrDetail {
    number: u64,
    title: String,
    #[serde(default)]
    body: Option<String>,
    state: String,
    draft: Option<bool>,
    merged: Option<bool>,
    #[serde(default)]
    merge_commit_sha: Option<String>,
    user: GhUser,
    head: GhRef,
    base: GhRef,
    html_url: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    labels: Vec<GhLabel>,
    #[serde(default)]
    assignees: Vec<GhUser>,
    #[serde(default)]
    requested_reviewers: Vec<GhUser>,
    #[serde(default)]
    comments: u32,
    mergeable: Option<bool>,
    /// Present + non-null when auto-merge is armed. See `GhPr.auto_merge`.
    #[serde(default)]
    auto_merge: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct GhUser {
    login: String,
    #[serde(default)]
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GhRef {
    #[serde(rename = "ref")]
    ref_name: String,
    sha: String,
}

#[derive(Deserialize)]
struct GhLabel {
    name: String,
    color: String,
}

#[derive(Deserialize)]
struct GhComment {
    id: i64,
    user: GhUser,
    body: String,
    created_at: String,
}

/// GitHub `/issues/{n}/events` payload — a heterogeneous list of timeline
/// events. We parse only the fields we render; everything else is ignored.
#[derive(Deserialize)]
struct GhEvent {
    id: i64,
    /// "labeled" | "unlabeled" | "assigned" | "unassigned" | "closed" |
    /// "reopened" | "merged" | "renamed" | "head_ref_force_pushed" |
    /// "head_ref_deleted" | "head_ref_restored" | "review_requested" |
    /// "review_request_removed" | "convert_to_draft" | "ready_for_review" |
    /// "auto_merge_enabled" | "auto_merge_disabled" | "milestoned" |
    /// "demilestoned" | "locked" | "unlocked" | … (many more, fall through to system)
    event: String,
    #[serde(default)]
    actor: Option<GhUser>,
    created_at: String,
    #[serde(default)]
    label: Option<GhLabel>,
    #[serde(default)]
    assignee: Option<GhUser>,
    #[serde(default)]
    requested_reviewer: Option<GhUser>,
    #[serde(default)]
    rename: Option<GhRename>,
}

#[derive(Deserialize)]
struct GhRename {
    from: String,
    to: String,
}

#[derive(Deserialize)]
struct GhPrFile {
    filename: String,
    status: String,
    additions: u32,
    deletions: u32,
    #[serde(default)]
    patch: Option<String>,
}

#[derive(Deserialize)]
struct GhPrCommit {
    sha: String,
    commit: GhCommitInner,
    html_url: String,
}

#[derive(Deserialize)]
struct GhCommitInner {
    message: String,
    author: GhCommitAuthor,
}

#[derive(Deserialize)]
struct GhCommitAuthor {
    name: String,
    date: String,
}

#[derive(Deserialize)]
struct GhCommitResponse {
    #[serde(default)]
    files: Vec<GhPrFile>,
}

// ---------------------------------------------------------------------------
// Mappers (copied verbatim)
// ---------------------------------------------------------------------------

fn github_pr_to_mr(p: GhPr, _owner: &str, _repo: &str) -> MergeRequest {
    let state = match p.state.as_str() {
        "closed" if p.merged_at.is_some() => MrState::Merged,
        "closed" => MrState::Closed,
        _ => MrState::Open,
    };
    let auto_merge_enabled = p.auto_merge.as_ref().is_some_and(|v| !v.is_null());
    MergeRequest {
        number: p.number,
        title: p.title,
        description: p.body.unwrap_or_default(),
        state,
        is_draft: p.draft.unwrap_or(false),
        author: gh_user_to_mr(p.user),
        source_branch: p.head.ref_name,
        target_branch: p.base.ref_name,
        web_url: p.html_url,
        created_at: p.created_at,
        updated_at: p.updated_at,
        labels: p
            .labels
            .into_iter()
            .map(|l| MrLabel { name: l.name, color: l.color })
            .collect(),
        assignees: p.assignees.into_iter().map(gh_user_to_mr).collect(),
        reviewers: p.requested_reviewers.into_iter().map(gh_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable: None,
        provider: "github".into(),
        comments_count: p.comments,
        squash: false,
        delete_branch: false,
        merge_commit_sha: p.merge_commit_sha,
        head_sha: p.head.sha,
        base_sha: p.base.sha,
        auto_merge_enabled,
    }
}

fn github_pr_detail_to_mr(p: GhPrDetail, _owner: &str, _repo: &str) -> MrDetail {
    let state = match (p.state.as_str(), p.merged) {
        (_, Some(true)) => MrState::Merged,
        ("closed", _) => MrState::Closed,
        _ => MrState::Open,
    };
    let auto_merge_enabled = p.auto_merge.as_ref().is_some_and(|v| !v.is_null());
    let mr = MergeRequest {
        number: p.number,
        title: p.title,
        description: p.body.unwrap_or_default(),
        state,
        is_draft: p.draft.unwrap_or(false),
        author: gh_user_to_mr(p.user),
        source_branch: p.head.ref_name,
        target_branch: p.base.ref_name,
        web_url: p.html_url,
        created_at: p.created_at,
        updated_at: p.updated_at,
        labels: p
            .labels
            .into_iter()
            .map(|l| MrLabel { name: l.name, color: l.color })
            .collect(),
        assignees: p.assignees.into_iter().map(gh_user_to_mr).collect(),
        reviewers: p.requested_reviewers.into_iter().map(gh_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable: p.mergeable,
        provider: "github".into(),
        comments_count: p.comments,
        squash: false,
        delete_branch: false,
        merge_commit_sha: p.merge_commit_sha,
        head_sha: p.head.sha,
        base_sha: p.base.sha,
        auto_merge_enabled,
    };
    MrDetail { mr, comments: vec![], events: vec![], checks: vec![] }
}

fn gh_user_to_mr(u: GhUser) -> MrUser {
    MrUser {
        login: u.login.clone(),
        display_name: u.name.unwrap_or(u.login),
        avatar_url: u.avatar_url,
    }
}

fn gh_comment_to_mr(c: GhComment) -> MrComment {
    let is_bot = is_bot_login(&c.user.login);
    MrComment {
        id: c.id.to_string(),
        author: gh_user_to_mr(c.user),
        body: c.body,
        created_at: c.created_at,
        is_bot,
    }
}

/// Convert a GitHub issue/PR event into an `MrEvent`. Returns None for
/// event types we don't surface (e.g. `subscribed`, `mentioned`, `referenced`),
/// which are mostly noise on the timeline.
fn gh_event_to_mr(e: GhEvent) -> Option<MrEvent> {
    // The "ghost" actor is GitHub's placeholder for deleted users.
    let actor = e.actor.map(gh_user_to_mr).unwrap_or_else(|| MrUser {
        login: "ghost".into(),
        display_name: "Unknown".into(),
        avatar_url: None,
    });

    let (kind, summary) = match e.event.as_str() {
        "labeled" => (
            "label",
            format!(
                "added the **{}** label",
                e.label.as_ref().map(|l| l.name.as_str()).unwrap_or("?")
            ),
        ),
        "unlabeled" => (
            "label",
            format!(
                "removed the **{}** label",
                e.label.as_ref().map(|l| l.name.as_str()).unwrap_or("?")
            ),
        ),
        "assigned" => (
            "assign",
            format!(
                "assigned **{}**",
                e.assignee.as_ref().map(|u| u.login.as_str()).unwrap_or("someone")
            ),
        ),
        "unassigned" => (
            "assign",
            format!(
                "unassigned **{}**",
                e.assignee.as_ref().map(|u| u.login.as_str()).unwrap_or("someone")
            ),
        ),
        "review_requested" => (
            "review",
            format!(
                "requested a review from **{}**",
                e.requested_reviewer.as_ref().map(|u| u.login.as_str()).unwrap_or("someone")
            ),
        ),
        "review_request_removed" => (
            "review",
            format!(
                "removed review request from **{}**",
                e.requested_reviewer.as_ref().map(|u| u.login.as_str()).unwrap_or("someone")
            ),
        ),
        "closed" => ("state", "closed this".to_string()),
        "reopened" => ("state", "reopened this".to_string()),
        "merged" => ("state", "merged this".to_string()),
        "convert_to_draft" => ("state", "marked this as a draft".to_string()),
        "ready_for_review" => ("state", "marked this as ready for review".to_string()),
        "auto_merge_enabled" => ("state", "enabled auto-merge".to_string()),
        "auto_merge_disabled" => ("state", "disabled auto-merge".to_string()),
        "head_ref_force_pushed" => ("commit", "force-pushed the source branch".to_string()),
        "head_ref_deleted" => ("commit", "deleted the source branch".to_string()),
        "head_ref_restored" => ("commit", "restored the source branch".to_string()),
        "renamed" => {
            let r = e.rename.as_ref();
            let from = r.map(|x| x.from.as_str()).unwrap_or("?");
            let to = r.map(|x| x.to.as_str()).unwrap_or("?");
            ("rename", format!("renamed from “{from}” to “{to}”"))
        }
        "milestoned" => ("system", "added a milestone".to_string()),
        "demilestoned" => ("system", "removed a milestone".to_string()),
        "locked" => ("system", "locked the conversation".to_string()),
        "unlocked" => ("system", "unlocked the conversation".to_string()),
        // Drop noisy / non-actionable events outright.
        "subscribed" | "unsubscribed" | "mentioned" | "referenced" | "head_ref_cleaned"
        | "marked_as_duplicate" | "unmarked_as_duplicate" => return None,
        _ => return None,
    };

    Some(MrEvent {
        id: e.id.to_string(),
        kind: kind.to_string(),
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
