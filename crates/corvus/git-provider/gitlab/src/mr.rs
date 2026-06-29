//! GitLab Merge Request operations.
//!
//! Folds the old `git_provider::gitlab::mr` delegate (RepoRef/MrId
//! destructuring, `map_state`, the project-path helper, and the
//! `add_mr_comment` placeholder) together with the REST bodies that lived in
//! `git_provider::mr_impl` (`list_gitlab_mrs`, `get_gitlab_mr`, …). Behavior is
//! preserved byte-for-byte: every error string, header, URL, query param, HTTP
//! method and status check is identical to the originals — only the auth/refresh
//! plumbing now goes through [`GitlabHttp`] and `AppError::Other(msg)` became
//! `classify(msg)`.

use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::{classify, percent_encode_slash, GitlabHttp};

// ---------------------------------------------------------------------------
// Delegate helpers (destructuring / validation)
// ---------------------------------------------------------------------------

fn project_path(repo: &RepoRef) -> &str {
    // GitLab convention: full project path is in `owner_or_path`; `name` unset.
    repo.owner_or_path.as_str()
}

fn id_parts(id: &MrId) -> Result<(&str, u64), ProviderError> {
    if !matches!(id.provider, ProviderKind::GitLab) {
        return Err(ProviderError::BadRequest(
            "MrId provider mismatch (expected GitLab)".into(),
        ));
    }
    Ok((id.owner_or_path.as_str(), id.number))
}

fn map_state(s: Option<&str>) -> &'static str {
    match s {
        Some("closed") => "closed",
        Some("merged") => "merged",
        Some("all") => "all",
        _ => "opened",
    }
}

// ---------------------------------------------------------------------------
// Trait-surface free functions (delegate behavior, REST inlined)
// ---------------------------------------------------------------------------

pub(crate) async fn list_mrs(
    http: &GitlabHttp,
    repo: &RepoRef,
    filter: MrFilter,
) -> Result<Vec<MergeRequest>, ProviderError> {
    let path = project_path(repo);
    let state = map_state(filter.state.as_deref());
    list_gitlab_mrs(http, path, state).await
}

pub(crate) async fn get_mr(http: &GitlabHttp, id: &MrId) -> Result<MrDetail, ProviderError> {
    let (path, iid) = id_parts(id)?;
    get_gitlab_mr(http, path, iid).await
}

pub(crate) async fn create_mr(
    http: &GitlabHttp,
    repo: &RepoRef,
    req: CreateMrParams,
) -> Result<MergeRequest, ProviderError> {
    let path = project_path(repo);
    create_gitlab_mr(http, path, &req).await
}

pub(crate) async fn update_mr(
    _http: &GitlabHttp,
    _id: &MrId,
    _req: MrUpdateRequest,
) -> Result<MergeRequest, ProviderError> {
    Err(ProviderError::Unsupported { feature: "update_mr".into() })
}

pub(crate) async fn close_mr(http: &GitlabHttp, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    update_gitlab_mr_state(http, path, iid, "close").await
}

pub(crate) async fn reopen_mr(http: &GitlabHttp, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    update_gitlab_mr_state(http, path, iid, "reopen").await
}

pub(crate) async fn merge_mr(
    http: &GitlabHttp,
    id: &MrId,
    opts: MergeOpts,
) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    merge_gitlab_mr(http, path, iid, opts.squash, opts.delete_branch).await
}

pub(crate) async fn list_mr_comments(
    _http: &GitlabHttp,
    _id: &MrId,
) -> Result<Vec<MrComment>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_comments (use get_mr)".into() })
}

pub(crate) async fn add_mr_comment(
    http: &GitlabHttp,
    id: &MrId,
    body: &str,
) -> Result<MrComment, ProviderError> {
    let (path, iid) = id_parts(id)?;
    add_gitlab_mr_note(http, path, iid, body).await?;
    // GitLab's note POST does return the created note, but the existing
    // helper discards it — Phase 5 will fix this. For now return a minimal
    // placeholder so the trait surface is honored.
    Ok(MrComment {
        id: "0".into(),
        author: MrUser {
            login: String::new(),
            display_name: String::new(),
            avatar_url: None,
        },
        body: body.into(),
        created_at: String::new(),
        is_bot: false,
    })
}

pub(crate) async fn list_mr_files(
    http: &GitlabHttp,
    id: &MrId,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let (path, iid) = id_parts(id)?;
    get_gitlab_mr_files(http, path, iid).await
}

pub(crate) async fn fetch_mr_diff(
    _http: &GitlabHttp,
    _id: &MrId,
) -> Result<String, ProviderError> {
    Err(ProviderError::Unsupported { feature: "fetch_mr_diff (use list_mr_files)".into() })
}

pub(crate) async fn check_mr_conflict(
    _http: &GitlabHttp,
    _id: &MrId,
) -> Result<MrConflict, ProviderError> {
    Err(ProviderError::Unsupported { feature: "check_mr_conflict".into() })
}

pub(crate) async fn list_mr_reviewers(
    _http: &GitlabHttp,
    _id: &MrId,
) -> Result<Vec<ProviderUser>, ProviderError> {
    Err(ProviderError::Unsupported { feature: "list_mr_reviewers".into() })
}

pub(crate) async fn request_mr_review(
    _http: &GitlabHttp,
    _id: &MrId,
    _user: &str,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "request_mr_review".into() })
}

pub(crate) async fn approve_mr(
    _http: &GitlabHttp,
    _id: &MrId,
) -> Result<(), ProviderError> {
    Err(ProviderError::Unsupported { feature: "approve_mr".into() })
}

pub(crate) async fn list_mr_commits(
    http: &GitlabHttp,
    id: &MrId,
) -> Result<Vec<MrCommit>, ProviderError> {
    let (path, iid) = id_parts(id)?;
    get_gitlab_mr_commits(http, path, iid).await
}

pub(crate) async fn get_commit_diff(
    http: &GitlabHttp,
    repo: &RepoRef,
    sha: &str,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let path = project_path(repo);
    get_gitlab_commit_files(http, path, sha).await
}

pub(crate) async fn mark_mr_ready(http: &GitlabHttp, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    mark_gitlab_mr_ready(http, path, iid).await
}

pub(crate) async fn enable_auto_merge(
    http: &GitlabHttp,
    id: &MrId,
    opts: AutoMergeOpts,
) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    // GitLab needs the MR's merge_status resolved before MWPS can be armed
    // reliably; poll until it settles (best-effort, then attempt the merge).
    wait_gitlab_merge_status_ready(http, path, iid).await;
    enable_gitlab_auto_merge(http, path, iid, opts.squash, opts.delete_branch).await
}

pub(crate) async fn disable_auto_merge(http: &GitlabHttp, id: &MrId) -> Result<(), ProviderError> {
    let (path, iid) = id_parts(id)?;
    disable_gitlab_auto_merge(http, path, iid).await
}

pub(crate) async fn auto_merge_allowed(
    http: &GitlabHttp,
    repo: &RepoRef,
) -> Result<MrCapabilities, ProviderError> {
    let path = project_path(repo);
    let supported = fetch_gitlab_mwps_supported(http, path).await?;
    if supported {
        Ok(MrCapabilities::default())
    } else {
        Ok(MrCapabilities {
            auto_merge_supported: false,
            auto_merge_reason: Some(
                "CI jobs are disabled for this project — there is no \
                 pipeline to wait on, so merge-when-pipeline-succeeds \
                 cannot be armed."
                    .into(),
            ),
        })
    }
}

pub(crate) async fn mr_feature_status(
    http: &GitlabHttp,
    repo: &RepoRef,
) -> Result<MrFeatureStatus, ProviderError> {
    let path = project_path(repo);
    fetch_gitlab_mr_feature_enabled(http, path).await
}

// ---------------------------------------------------------------------------
// REST functions (ported verbatim from mr_impl, via GitlabHttp)
// ---------------------------------------------------------------------------

pub(crate) async fn list_gitlab_mrs(
    http: &GitlabHttp,
    project_path: &str,
    state: &str, // "opened" | "closed" | "merged" | "all"
) -> Result<Vec<MergeRequest>, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests?state={state}&per_page=50&order_by=updated_at&sort=desc",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab API {s}: {b}")));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| classify(format!("GitLab MR body read error: {e}")))?;
    let mrs: Vec<GlMr> = serde_json::from_str(&body).map_err(|e| {
        classify(format!(
            "GitLab MR parse: {e} — body snippet: {}",
            &body[..body.len().min(400)]
        ))
    })?;
    Ok(mrs.into_iter().map(gitlab_mr_to_mr).collect())
}

pub(crate) async fn get_gitlab_mr(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
) -> Result<MrDetail, ProviderError> {
    let encoded = percent_encode_slash(project_path);

    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab API {s}: {b}")));
    }
    let mr: GlMr = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab MR detail parse: {e}")))?;
    let mr_converted = gitlab_mr_to_mr(mr);

    // Fetch notes
    let notes_url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}/notes?sort=asc&order_by=created_at&per_page=50",
        http.base()
    );
    let notes_resp = http
        .send(|s| {
            http.client()
                .get(&notes_url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await;

    let (comments, events): (Vec<MrComment>, Vec<MrEvent>) = match notes_resp {
        Ok(r) if r.status().is_success() => {
            let raw: Vec<GlNote> = r.json().await.unwrap_or_default();
            // Partition: user-authored notes → comments, system notes → events.
            let mut comments = Vec::new();
            let mut events = Vec::new();
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
        mr: mr_converted,
        comments,
        events,
        checks: vec![],
    })
}

pub(crate) async fn create_gitlab_mr(
    http: &GitlabHttp,
    project_path: &str,
    params: &CreateMrParams,
) -> Result<MergeRequest, ProviderError> {
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

    let url_create = format!("{}/api/v4/projects/{encoded}/merge_requests", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .post(&url_create)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(gitlab_create_mr_error(s, b, params));
    }
    let mr: GlMr = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab create MR parse: {e}")))?;
    Ok(gitlab_mr_to_mr(mr))
}

/// Map a GitLab "create MR" failure to a human-readable message.  GitLab
/// returns errors as `{"message": {"field": ["reason", ...]}}` or
/// `{"message": "..."}` depending on the validation layer.
fn gitlab_create_mr_error(
    status: reqwest::StatusCode,
    body: String,
    params: &CreateMrParams,
) -> ProviderError {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
        let msg_node = v.get("message").or_else(|| v.get("error"));
        // Flat string message.
        if let Some(s) = msg_node.and_then(|m| m.as_str()) {
            let low = s.to_lowercase();
            if low.contains("source") && low.contains("does not exist") {
                return classify(format!(
                    "Source branch '{}' was not found on the GitLab remote. \
                     Push it to origin first, then try again.",
                    params.source_branch,
                ));
            }
            if low.contains("target") && low.contains("does not exist") {
                return classify(format!(
                    "Target branch '{}' does not exist on the GitLab remote.",
                    params.target_branch,
                ));
            }
            if low.contains("another open merge request already exists")
                || low.contains("already exists")
            {
                return classify(format!(
                    "A merge request already exists for '{}' → '{}'.",
                    params.source_branch, params.target_branch,
                ));
            }
            if !s.is_empty() {
                return classify(format!("GitLab: {s}"));
            }
        }
        // Nested error object: `{"message": {"source_branch": ["can't be blank"]}}`.
        if let Some(obj) = msg_node.and_then(|m| m.as_object()) {
            let source_err = obj
                .get("source_branch")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str());
            if let Some(err) = source_err {
                return classify(format!(
                    "Source branch '{}' — {err}. \
                     If it hasn't been pushed yet, push it to origin first.",
                    params.source_branch,
                ));
            }
            let target_err = obj
                .get("target_branch")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str());
            if let Some(err) = target_err {
                return classify(format!(
                    "Target branch '{}' — {err}.",
                    params.target_branch,
                ));
            }
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return classify(
                "GitLab refused the request — check your token permissions in \
                 Settings → Git & Integrations."
                    .into(),
            );
        }
    }
    let trimmed: String = body.chars().take(300).collect();
    classify(format!("GitLab create MR {status}: {trimmed}"))
}

pub(crate) async fn merge_gitlab_mr(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
    squash: bool,
    delete_branch: bool,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let body = serde_json::json!({
        "squash":                      squash,
        "should_remove_source_branch": delete_branch,
    });
    let url_merge = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}/merge",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .put(&url_merge)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab merge MR {s}: {b}")))
}

/// Query the GitLab project endpoint and check whether MWPS can be armed.
/// We approximate "MWPS supported" with `jobs_enabled` — a project without
/// CI cannot have a pipeline to wait on, so MWPS would degrade to an
/// immediate merge or a 405.
pub(crate) async fn fetch_gitlab_mwps_supported(
    http: &GitlabHttp,
    project_path: &str,
) -> Result<bool, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{}/api/v4/projects/{encoded}", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab project fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab project parse: {e}")))?;
    Ok(v.get("jobs_enabled").and_then(|b| b.as_bool()).unwrap_or(true))
}

/// GitLab probe: `merge_requests_access_level = "disabled"` means the
/// MR feature has been turned off in project settings, so every MR call
/// returns 404.
pub(crate) async fn fetch_gitlab_mr_feature_enabled(
    http: &GitlabHttp,
    project_path: &str,
) -> Result<MrFeatureStatus, ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!("{}/api/v4/projects/{encoded}", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab project fetch {s}: {b}")));
    }
    let v: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab project parse: {e}")))?;
    let access = v
        .get("merge_requests_access_level")
        .and_then(|s| s.as_str())
        .unwrap_or("enabled");
    if access == "disabled" {
        return Ok(MrFeatureStatus {
            enabled: false,
            reason: Some(
                "Merge requests are disabled in this project's settings on GitLab.".into(),
            ),
        });
    }
    Ok(MrFeatureStatus::default())
}

pub(crate) async fn update_gitlab_mr_state(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
    event: &str, // "close" | "reopen"
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let body = serde_json::json!({ "state_event": event });
    let url_state = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .put(&url_state)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab update MR {s}: {b}")))
}

pub(crate) async fn add_gitlab_mr_note(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
    body: &str,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let payload = serde_json::json!({ "body": body });
    let url_note = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}/notes",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .post(&url_note)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&payload)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab add note {s}: {b}")))
}

pub(crate) async fn get_gitlab_mr_files(
    http: &GitlabHttp,
    path: &str,
    number: u64,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let encoded = percent_encode_slash(path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{number}/diffs?per_page=100",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab MR diffs {s}: {b}")));
    }
    let diffs: Vec<GlMrDiff> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab MR diffs parse: {e}")))?;
    Ok(diffs
        .into_iter()
        .map(gl_diff_to_file)
        .collect())
}

pub(crate) async fn get_gitlab_mr_commits(
    http: &GitlabHttp,
    path: &str,
    number: u64,
) -> Result<Vec<MrCommit>, ProviderError> {
    let encoded = percent_encode_slash(path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{number}/commits?per_page=100",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab MR commits {s}: {b}")));
    }
    let commits: Vec<GlMrCommitItem> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab MR commits parse: {e}")))?;
    Ok(commits
        .into_iter()
        .map(|c| MrCommit {
            sha: c.id.clone(),
            message: c.title,
            author: c.author_name,
            date: c.created_at,
            web_url: c.web_url,
        })
        .collect())
}

/// Per-file diff for a single commit SHA (Commits-tab drill-down).
pub(crate) async fn get_gitlab_commit_files(
    http: &GitlabHttp,
    path: &str,
    sha: &str,
) -> Result<Vec<MrFileDiff>, ProviderError> {
    let encoded = percent_encode_slash(path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/repository/commits/{sha}/diff",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(classify(format!("GitLab commit diff {s}: {b}")));
    }
    let diffs: Vec<GlMrDiff> = resp
        .json()
        .await
        .map_err(|e| classify(format!("GitLab commit diff parse: {e}")))?;
    Ok(diffs
        .into_iter()
        .map(gl_diff_to_file)
        .collect())
}

/// Mark a GitLab MR as ready for review (removes Draft prefix).
pub(crate) async fn mark_gitlab_mr_ready(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    // GitLab API supports draft:false directly since v14.x
    let body = serde_json::json!({ "draft": false });
    let url_ready = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .put(&url_ready)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab mark ready {s}: {b}")))
}

/// Poll the MR until GitLab finishes computing `merge_status` (it starts as
/// `checking`/`unchecked` right after creation). Returns once the status is
/// resolved, or after the timeout — callers should still attempt the merge
/// either way so transient API hiccups don't block the user.
pub(crate) async fn wait_gitlab_merge_status_ready(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
) {
    #[derive(Deserialize)]
    struct StatusOnly {
        merge_status: Option<String>,
    }

    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}",
        http.base()
    );

    let delays_ms = [400u64, 600, 800, 1200, 1500, 1500];
    for delay in delays_ms {
        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        let resp = http
            .send(|s| {
                http.client()
                    .get(&url)
                    .header("Authorization", &s.auth_header)
                    .header("User-Agent", "arbor-git-gui/1.0")
            })
            .await;
        let Ok(r) = resp else { continue };
        if !r.status().is_success() {
            continue;
        }
        let Ok(s) = r.json::<StatusOnly>().await else {
            continue;
        };
        match s.merge_status.as_deref() {
            Some("checking") | Some("unchecked") | None => continue,
            _ => return,
        }
    }
}

/// Enable "merge when pipeline succeeds" on a GitLab MR.
/// When no pipeline exists this endpoint merges immediately, so upstream code
/// should only call this when the MR has CI configured.  Any failure is
/// bubbled up as an error so the caller can notify the user.
pub(crate) async fn enable_gitlab_auto_merge(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
    squash: bool,
    delete_branch: bool,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let body = serde_json::json!({
        "merge_when_pipeline_succeeds": true,
        "squash":                       squash,
        "should_remove_source_branch":  delete_branch,
    });
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}/merge",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .put(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .json(&body)
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(map_auto_merge_error(&b)
        .unwrap_or_else(|| classify(format!("GitLab auto-merge {s}: {b}"))))
}

/// Cancel "merge when pipeline succeeds" on a GitLab MR.
/// Idempotent — the endpoint returns 200 even if MWPS isn't currently armed.
pub(crate) async fn disable_gitlab_auto_merge(
    http: &GitlabHttp,
    project_path: &str,
    iid: u64,
) -> Result<(), ProviderError> {
    let encoded = percent_encode_slash(project_path);
    let url = format!(
        "{}/api/v4/projects/{encoded}/merge_requests/{iid}/cancel_merge_when_pipeline_succeeds",
        http.base()
    );
    let resp = http
        .send(|s| {
            http.client()
                .post(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
                .header("Content-Length", "0")
        })
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let s = resp.status();
    let b = resp.text().await.unwrap_or_default();
    Err(classify(format!("GitLab cancel-MWPS {s}: {b}")))
}

/// Detect the well-known "MR is not mergeable" failure modes from a raw
/// provider error message and re-phrase them so the user understands *why*
/// auto-merge couldn't be armed. Returns `None` when the message doesn't match
/// any recognised shape — the caller falls back to surfacing the raw response.
///
/// GitLab variants (REST `merge` endpoint response body):
///   - "Branch cannot be merged"
///   - "merge request is not mergeable"
///   - JSON {"message":"406 Branch cannot be merged"} / {"message":"...conflict..."}
fn map_auto_merge_error(raw: &str) -> Option<ProviderError> {
    // Try JSON first — GitLab wraps the message.
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
        return Some(classify(
            "This pull request has conflicts that must be resolved before \
             auto-merge can be enabled. Rebase or merge the target branch in, \
             fix the conflicts, then push.".into()
        ));
    }
    if lower.contains("clean status") {
        return Some(classify(
            "Auto-merge needs a pending check or required review to gate on. \
             This pull request is already mergeable — merge it directly instead.".into()
        ));
    }
    if lower.contains("auto_merge") && lower.contains("disabled") {
        return Some(classify(
            "Auto-merge is disabled for this repository. Enable it in the \
             repository settings, then try again.".into()
        ));
    }
    None
}

// ---------------------------------------------------------------------------
// Bot heuristic
// ---------------------------------------------------------------------------

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
    iid: u64,
    title: String,
    #[serde(default)]
    description: Option<String>,
    state: String,
    author: GlUser,
    source_branch: String,
    target_branch: String,
    web_url: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    assignees: Vec<GlUser>,
    #[serde(default)]
    reviewers: Vec<GlUser>,
    #[serde(default)]
    user_notes_count: u32,
    #[serde(default)]
    merge_status: Option<String>,
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
    merge_commit_sha: Option<String>,
    /// Present only for squash merges; takes precedence over merge_commit_sha.
    #[serde(default)]
    squash_commit_sha: Option<String>,
    /// SHA of the source branch HEAD at last update (GitLab's top-level "sha" field).
    #[serde(default)]
    sha: Option<String>,
}

#[derive(Deserialize)]
struct GlUser {
    username: String,
    name: String,
    avatar_url: Option<String>,
}

#[derive(Deserialize)]
struct GlNote {
    id: i64,
    author: GlUser,
    body: String,
    created_at: String,
    system: bool,
}

#[derive(Deserialize)]
struct GlMrDiff {
    new_path: String,
    new_file: bool,
    deleted_file: bool,
    renamed_file: bool,
    diff: String,
}

/// Map a raw GitLab diff entry to the wire `MrFileDiff`. Shared by the MR-level
/// diff endpoint and the single-commit diff endpoint — both return the same
/// `GlMrDiff` shape, so the status derivation + +/- line counting lives once.
fn gl_diff_to_file(d: GlMrDiff) -> MrFileDiff {
    let status = if d.new_file {
        "added".into()
    } else if d.deleted_file {
        "removed".into()
    } else if d.renamed_file {
        "renamed".into()
    } else {
        "modified".into()
    };
    let additions = d
        .diff
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .count() as u32;
    let deletions = d
        .diff
        .lines()
        .filter(|l| l.starts_with('-') && !l.starts_with("---"))
        .count() as u32;
    MrFileDiff {
        filename: d.new_path,
        status,
        additions,
        deletions,
        patch: if d.diff.is_empty() { None } else { Some(d.diff) },
    }
}

#[derive(Deserialize)]
struct GlMrCommitItem {
    id: String,
    title: String,
    author_name: String,
    created_at: String,
    #[serde(default)]
    web_url: Option<String>,
}

// ---------------------------------------------------------------------------
// Mappers (copied verbatim)
// ---------------------------------------------------------------------------

fn gitlab_mr_to_mr(m: GlMr) -> MergeRequest {
    let state = match m.state.as_str() {
        "merged" => MrState::Merged,
        "closed" => MrState::Closed,
        _ => MrState::Open,
    };
    let mergeable = match m.merge_status.as_deref() {
        Some("can_be_merged") => Some(true),
        Some("cannot_be_merged") | Some("cannot_be_merged_recheck") => Some(false),
        _ => None,
    };
    let is_draft = m.draft || m.work_in_progress;
    MergeRequest {
        number: m.iid,
        title: m.title,
        description: m.description.unwrap_or_default(),
        state,
        is_draft,
        author: gl_user_to_mr(m.author),
        source_branch: m.source_branch,
        target_branch: m.target_branch,
        web_url: m.web_url,
        created_at: m.created_at,
        updated_at: m.updated_at,
        labels: m
            .labels
            .into_iter()
            .map(|l| MrLabel { name: l.clone(), color: "6f7178".into() })
            .collect(),
        assignees: m.assignees.into_iter().map(gl_user_to_mr).collect(),
        reviewers: m.reviewers.into_iter().map(gl_user_to_mr).collect(),
        checks_status: "none".into(),
        mergeable,
        provider: "gitlab".into(),
        comments_count: m.user_notes_count,
        squash: m.squash,
        delete_branch: m.should_remove_source_branch || m.force_remove_source_branch,
        // For squash merges prefer squash_commit_sha; fall back to merge_commit_sha.
        merge_commit_sha: m.squash_commit_sha.or(m.merge_commit_sha),
        head_sha: m.sha.unwrap_or_default(),
        base_sha: String::new(), // GitLab API doesn't expose base SHA in list response
        auto_merge_enabled: m.merge_when_pipeline_succeeds,
    }
}

fn gl_user_to_mr(u: GlUser) -> MrUser {
    MrUser {
        login: u.username,
        display_name: u.name,
        avatar_url: u.avatar_url,
    }
}

fn gl_note_to_comment(n: GlNote) -> MrComment {
    let is_bot = is_bot_user_gl(&n.author.username, &n.author.name);
    MrComment {
        id: n.id.to_string(),
        author: gl_user_to_mr(n.author),
        body: n.body,
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
    let kind = if lower.contains("approved") {
        // Also catches "unapproved" — it contains the "approved" substring.
        "review"
    } else if lower.contains("assigned") {
        "assign"
    } else if lower.contains("requested review") || lower.contains("review requested") {
        "review"
    } else if lower.contains("added") && lower.contains("label")
        || lower.contains("removed") && lower.contains("label")
        || lower.contains("scoped label")
    {
        "label"
    } else if lower.contains("added") && lower.contains("commit")
        || lower.contains("force-pushed")
        || lower.contains("pushed ")
    {
        "commit"
    } else if lower.contains("merged")
        || lower.contains("closed")
        || lower.contains("reopened")
        || lower.contains("marked") && lower.contains("draft")
        || lower.contains("marked") && lower.contains("ready")
    {
        "state"
    } else if lower.contains("changed title") || lower.contains("changed description") {
        "rename"
    } else {
        "system"
    };
    MrEvent {
        id: n.id.to_string(),
        kind: kind.to_string(),
        actor: gl_user_to_mr(n.author),
        summary: n.body,
        created_at: n.created_at,
    }
}
