//! GitLab OAuth flow — wraps the legacy `auth::oauth_gitlab` module.
//!
//! Mirrors `oauth/github.rs`: `start_oauth` requires a `tauri::AppHandle`
//! that the trait surface can't carry, so the trait method on
//! `GitlabProvider` returns `Unsupported`. The command layer (Phase 4)
//! drives the flow via the helpers exposed here.

use crate::git_provider::types::error::ProviderError;

pub fn revoke_token() -> Result<(), ProviderError> {
    crate::git_provider::oauth::gitlab_flow::disconnect()
        .map_err(|e| ProviderError::Internal(e.to_string()))
}

/// Kicks off the OAuth flow via the existing implementation.
/// Returns the auth URL the user must open in their browser.
pub async fn start(app: tauri::AppHandle) -> Result<String, ProviderError> {
    crate::git_provider::oauth::gitlab_flow::start_gitlab_oauth(app)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}
