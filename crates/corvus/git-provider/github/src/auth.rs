//! GitHub auth — current-user resolution.
//!
//! Token presence is handled by the provider via `GithubHttp::has_credentials`;
//! the OAuth flow stays in the shell. This module only ports `current_user`
//! (`GET /user`), routed through the injected session seam.

use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::GithubHttp;

pub(crate) async fn current_user(http: &GithubHttp) -> Result<ProviderUser, ProviderError> {
    let resp = http
        .send(|s| {
            http.client()
                .get("https://api.github.com/user")
                .header("Authorization", &s.auth_header)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status: status.as_u16(), body });
    }

    #[derive(Deserialize)]
    struct GhUser {
        id:         i64,
        login:      String,
        name:       Option<String>,
        email:      Option<String>,
        avatar_url: Option<String>,
        html_url:   Option<String>,
    }
    let u: GhUser = resp.json().await?;
    Ok(ProviderUser {
        id:         u.id.to_string(),
        login:      u.login,
        name:       u.name,
        email:      u.email,
        avatar_url: u.avatar_url,
        web_url:    u.html_url,
    })
}
