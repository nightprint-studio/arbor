//! GitHub auth — token presence + current-user resolution.
//!
//! Token lookups go through `api::get_token`; OAuth flow lives in
//! `git_provider::oauth::github`.

use serde::Deserialize;

use crate::git_provider::types::{ProviderUser, error::ProviderError};

use super::api;

pub fn has_token() -> bool {
    api::get_token().ok().flatten().is_some()
}

pub async fn current_user() -> Result<ProviderUser, ProviderError> {
    let token = api::get_token()
        .map_err(|e| ProviderError::Internal(format!("github token lookup: {e}")))?
        .ok_or(ProviderError::Unauthenticated)?;

    let client = reqwest::Client::new();
    let resp = api::github_send_with_refresh(
        |tok| client
            .get(format!("{}/user", api::GITHUB_API_BASE))
            .header("Authorization", format!("Bearer {tok}"))
            .header("Accept", api::ACCEPT_JSON)
            .header("X-GitHub-Api-Version", api::API_VERSION)
            .header("User-Agent", api::USER_AGENT),
        &token,
    )
    .await
    .map_err(|e| ProviderError::Network(e.to_string()))?;

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
