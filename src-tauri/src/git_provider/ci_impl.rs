use crate::error::{AppError, Result};

// OAuth refresh serialization lives next to the refresh implementation in
// `crate::git_provider::oauth::{gitlab_flow, github_flow}`. The
// `try_refresh_if_stale` helpers there acquire the lock and coalesce
// concurrent 401-driven refreshes for us — the senders below just call them.

// ---------------------------------------------------------------------------
// Public types — defined in `corvus-git-provider-api`, re-exported here so the
// detection helpers below and external `ci_impl::*` call sites keep resolving.
// CI REST behavior now lives behind the `GitProvider` trait (the github/gitlab
// crates); this module keeps only provider detection, token retrieval, and the
// shared 401-refresh senders still used by the avatar/commit-graph lookups.
// ---------------------------------------------------------------------------

pub use corvus_git_provider_api::ci::*;

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
