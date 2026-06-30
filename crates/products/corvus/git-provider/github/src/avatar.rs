//! GitHub commit-email → avatar lookup. Ported from the shell's
//! `git_provider::avatar_lookup::fetch_github`, but over this crate's keyring-free
//! [`GithubHttp`] (session + 401-refresh injected), so it works identically
//! in-process and out-of-process. The cache + machine-email skip live in the
//! shared `corvus_git_provider_api::avatar::resolve_avatar` wrapper.

use corvus_git_provider_api::prelude::ProviderError;
use serde::Deserialize;

use crate::http::GithubHttp;

const ACCEPT_JSON: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const USER_AGENT: &str = "arbor-git-gui/1.0";

#[derive(Deserialize)]
struct User {
    avatar_url: Option<String>,
}

/// Resolve `email` to an `avatar_url`. `Ok(None)` = no match (or a non-success
/// response); `Err` only on transport/decode failure (the caller treats either
/// as "no avatar").
pub(crate) async fn avatar_url_for_email(
    http: &GithubHttp,
    email: &str,
) -> Result<Option<String>, ProviderError> {
    // Fast path: `*.noreply.github.com` emails encode the username directly, so
    // we hit the cheap `GET /users/:username` instead of the rate-limited search.
    if let Some(username) = parse_noreply(email) {
        let path = format!("/users/{}", percent_encode(&username));
        let resp = http
            .send(|s| {
                http.client()
                    .get(format!("{}{path}", s.base_url))
                    .header("Authorization", &s.auth_header)
                    .header("Accept", ACCEPT_JSON)
                    .header("X-GitHub-Api-Version", API_VERSION)
                    .header("User-Agent", USER_AGENT)
            })
            .await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let u: User = resp
            .json()
            .await
            .map_err(|e| ProviderError::Internal(format!("github avatar decode: {e}")))?;
        return Ok(u.avatar_url);
    }

    // Slow path: search by public email. Only matches users who have made the
    // email public on their profile — coverage is partial by design.
    let q = format!("{email}+in:email");
    let path = format!("/search/users?q={}&per_page=1", percent_encode(&q));
    let resp = http
        .send(|s| {
            http.client()
                .get(format!("{}{path}", s.base_url))
                .header("Authorization", &s.auth_header)
                .header("Accept", ACCEPT_JSON)
                .header("X-GitHub-Api-Version", API_VERSION)
                .header("User-Agent", USER_AGENT)
        })
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }

    #[derive(Deserialize)]
    struct SearchResp {
        items: Vec<User>,
    }
    let body: SearchResp = resp
        .json()
        .await
        .map_err(|e| ProviderError::Internal(format!("github avatar decode: {e}")))?;
    Ok(body.items.into_iter().next().and_then(|u| u.avatar_url))
}

/// Parse `<id>+<username>@users.noreply.github.com` / `<username>@users.noreply.github.com`.
fn parse_noreply(email: &str) -> Option<String> {
    let lower = email.to_ascii_lowercase();
    let (local, _) = lower.split_once('@')?;
    if !lower.ends_with("@users.noreply.github.com") {
        return None;
    }
    let username = match local.split_once('+') {
        Some((_, name)) => name,
        None => local,
    };
    if username.is_empty() {
        None
    } else {
        Some(username.to_string())
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        let safe = matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~');
        if safe {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
