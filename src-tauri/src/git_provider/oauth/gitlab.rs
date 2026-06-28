//! GitLab OAuth flow — wraps the legacy `auth::oauth_gitlab` module.
//!
//! Mirrors `oauth/github.rs`: the flow emits its completion event through an
//! [`EventSink`], so it runs from the `&AppState` provider handler without a
//! `tauri::AppHandle`.

use arbor_ipc::prelude::EventSink;

use corvus_git_provider_api::prelude::ProviderError;

/// Kicks off the OAuth flow via the existing implementation.
/// Returns the auth URL the user must open in their browser.
pub async fn start(sink: std::sync::Arc<dyn EventSink>) -> Result<String, ProviderError> {
    crate::git_provider::oauth::gitlab_flow::start_gitlab_oauth(sink)
        .await
        .map_err(|e| ProviderError::Internal(e.to_string()))
}
