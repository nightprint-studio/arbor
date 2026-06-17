//! Host-gated inline-image proxy for GitHub / GitLab MR/PR bodies & comments.
//!
//! This is deliberately NOT a `GitProvider` trait method: it is host-dynamic
//! (the target host comes from an arbitrary URL in an MR body, possibly a
//! self-hosted GitLab or a public CDN), and the token-attachment decision is
//! per-URL, not per-provider-instance. Routing it through the registry's
//! gitlab.com provider would attach the wrong token (or none) for self-hosted
//! instances. So it stays shell-side — mirroring how the issue-tracker image
//! fetch lives in `integrations::{linear,jira}::fetch_image_bytes`.
//!
//! The provider token is attached ONLY when the target host belongs to the
//! provider (github.com / the configured GitLab host). Public CDN assets — e.g.
//! `user-images.githubusercontent.com` for GitHub, or anything off-host — are
//! fetched anonymously so the token is never sent to a third party. Relative
//! GitLab `/uploads/...` URLs are resolved against `base_url`.

use crate::error::{AppError, Result};
use crate::git_provider::ci_impl::{
    get_github_token, get_gitlab_token, github_send_with_refresh, gitlab_send_with_refresh,
};

/// Fetch an image referenced inline by an MR/PR body or comment for preview.
pub async fn fetch_image_bytes(
    provider: &str,
    base_url: Option<&str>,
    url:      &str,
) -> Result<(Vec<u8>, Option<String>)> {
    match provider {
        "github" => fetch_github_image(url).await,
        "gitlab" => fetch_gitlab_image(base_url.unwrap_or("https://gitlab.com"), url).await,
        _ => Err(AppError::Other(format!("Unknown provider: {provider}"))),
    }
}

async fn fetch_github_image(url: &str) -> Result<(Vec<u8>, Option<String>)> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(AppError::Other("Relative GitHub image URLs are not supported".into()));
    }
    // Only github.com / api.github.com get the bearer token. `*.githubusercontent.com`
    // (user-images, objects/S3) are public and must NOT receive it.
    let is_github = reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()))
        .map(|h| h == "github.com" || h == "api.github.com")
        .unwrap_or(false);

    let client = reqwest::Client::new();
    let resp = if is_github {
        let token = get_github_token()?
            .ok_or_else(|| AppError::AuthFailed("Not connected to GitHub".into()))?;
        github_send_with_refresh(
            |tok| client
                .get(url)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", "*/*")
                .header("User-Agent", "arbor-git-gui/1.0"),
            &token,
        ).await?
    } else {
        client
            .get(url)
            .header("Accept", "*/*")
            .header("User-Agent", "arbor-git-gui/1.0")
            .send()
            .await
            .map_err(|e| AppError::Other(format!("Image request failed: {e}")))?
    };
    image_resp_to_bytes(resp).await
}

async fn fetch_gitlab_image(base_url: &str, url: &str) -> Result<(Vec<u8>, Option<String>)> {
    let base = base_url.trim_end_matches('/');
    let abs = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else if url.starts_with('/') {
        format!("{base}{url}")
    } else {
        format!("{base}/{url}")
    };

    let base_host = reqwest::Url::parse(base)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()));
    let url_host = reqwest::Url::parse(&abs)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_lowercase()));
    let same_host = matches!((&base_host, &url_host), (Some(a), Some(b)) if a == b);

    let client = reqwest::Client::new();
    let resp = if same_host {
        let token = get_gitlab_token(base)?
            .ok_or_else(|| AppError::AuthFailed("Not connected to GitLab".into()))?;
        gitlab_send_with_refresh(
            |tok| client
                .get(&abs)
                .header("Authorization", format!("Bearer {tok}"))
                .header("Accept", "*/*")
                .header("User-Agent", "arbor-git-gui/1.0"),
            base,
            &token,
        ).await?
    } else {
        client
            .get(&abs)
            .header("Accept", "*/*")
            .header("User-Agent", "arbor-git-gui/1.0")
            .send()
            .await
            .map_err(|e| AppError::Other(format!("Image request failed: {e}")))?
    };
    image_resp_to_bytes(resp).await
}

async fn image_resp_to_bytes(resp: reqwest::Response) -> Result<(Vec<u8>, Option<String>)> {
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(AppError::AuthFailed("Image access denied".into()));
    }
    if !status.is_success() {
        return Err(AppError::Other(format!("Image HTTP {status}")));
    }
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::Other(format!("Image read: {e}")))?;
    Ok((bytes.to_vec(), ctype))
}
