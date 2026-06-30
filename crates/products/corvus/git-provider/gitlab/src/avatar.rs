//! GitLab commit-email → avatar lookup. Ported from the shell's
//! `git_provider::avatar_lookup::fetch_gitlab`, but over this crate's
//! keyring-free [`GitlabHttp`] (session + 401-refresh injected). It authenticates
//! with the injected `Authorization` header (Bearer) like every other GitLab REST
//! call in this crate — GitLab accepts it for both OAuth and PAT tokens. The
//! cache + machine-email skip live in the shared
//! `corvus_git_provider_api::avatar::resolve_avatar` wrapper.

use corvus_git_provider_api::prelude::ProviderError;
use serde::Deserialize;

use crate::http::GitlabHttp;

const USER_AGENT: &str = "arbor-git-gui/1.0";

#[derive(Deserialize)]
struct User {
    avatar_url: Option<String>,
}

/// Resolve `email` to an `avatar_url` via GitLab's `?search=` (matches name,
/// username and *public* email). `Ok(None)` = no match / non-success; `Err` only
/// on transport/decode failure.
pub(crate) async fn avatar_url_for_email(
    http: &GitlabHttp,
    email: &str,
) -> Result<Option<String>, ProviderError> {
    let path = format!("/api/v4/users?search={}&per_page=1", percent_encode(email));
    let resp = http
        .send(|s| {
            http.client()
                .get(format!("{}{path}", s.base_url))
                .header("Authorization", &s.auth_header)
                .header("User-Agent", USER_AGENT)
        })
        .await?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: Vec<User> = resp
        .json()
        .await
        .map_err(|e| ProviderError::Internal(format!("gitlab avatar decode: {e}")))?;
    Ok(body.into_iter().next().and_then(|u| u.avatar_url))
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
