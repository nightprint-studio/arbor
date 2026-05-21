//! GitLab auth — token presence + current-user resolution.

use serde::Deserialize;

use crate::git_provider::types::{ProviderUser, error::ProviderError};

use super::api;

pub fn has_token(base_url: &str) -> bool {
    api::get_token(base_url).ok().flatten().is_some()
}

pub async fn current_user(base_url: &str) -> Result<ProviderUser, ProviderError> {
    let token = api::get_token(base_url)
        .map_err(|e| ProviderError::Internal(format!("gitlab token lookup: {e}")))?
        .ok_or(ProviderError::Unauthenticated)?;

    let url = format!("{base_url}/api/v4/user");
    let client = reqwest::Client::new();
    let resp = api::gitlab_send_with_refresh(
        |tok| client
            .get(&url)
            .header("Authorization", format!("Bearer {tok}"))
            .header("User-Agent", api::USER_AGENT),
        base_url,
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
