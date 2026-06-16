//! GitLab auth — current-user resolution.
//!
//! Token presence is handled by the provider via `GitlabHttp::has_credentials`;
//! the OAuth flow stays in the shell. This module only ports `current_user`
//! (`GET /api/v4/user`), routed through the injected session seam.

use serde::Deserialize;

use corvus_git_provider_api::prelude::*;

use crate::http::GitlabHttp;

pub(crate) async fn current_user(http: &GitlabHttp) -> Result<ProviderUser, ProviderError> {
    let url = format!("{}/api/v4/user", http.base());
    let resp = http
        .send(|s| {
            http.client()
                .get(&url)
                .header("Authorization", &s.auth_header)
                .header("User-Agent", "arbor-git-gui/1.0")
        })
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::Http { status: status.as_u16(), body });
    }

    #[derive(Deserialize)]
    struct GlUser {
        id:         i64,
        username:   String,
        name:       Option<String>,
        email:      Option<String>,
        avatar_url: Option<String>,
        web_url:    Option<String>,
    }
    let u: GlUser = resp.json().await?;
    Ok(ProviderUser {
        id:         u.id.to_string(),
        login:      u.username,
        name:       u.name,
        email:      u.email,
        avatar_url: u.avatar_url,
        web_url:    u.web_url,
    })
}
