pub mod credential_store;
pub mod oauth_jira;
pub mod oauth_linear;

// `oauth_github` and `oauth_gitlab` were relocated to
// `crate::git_provider::oauth::{github_flow, gitlab_flow}` in Phase 5 of the
// GitProvider refactor.  Aliases below keep the legacy paths working for
// internal callers (`auth::maybe_refresh_for_*`) without forcing an
// auth-module rewrite.
pub use crate::git_provider::oauth::github_flow as oauth_github;
pub use crate::git_provider::oauth::gitlab_flow as oauth_gitlab;

use serde::{Deserialize, Serialize};

/// Attempt a silent token refresh for the OAuth provider that matches the
/// given remote URL.  Called before every fetch / push / pull so an expired
/// access token is replaced before libgit2 tries to use it.
///
/// Never returns an error — refresh failures are logged as warnings and the
/// caller proceeds with whatever token (or lack of token) is in the keychain.
/// If the token was refreshed the keychain entry is updated in-place, so the
/// subsequent `resolve_credentials` call in `remote.rs` will pick up the new
/// value automatically.
pub async fn maybe_refresh_for_url(url: &str) {
    let host = credential_store::extract_host(url)
        .unwrap_or_default();

    if host.contains("github.com") {
        match oauth_github::try_refresh().await {
            Ok(true)  => tracing::debug!("github access token refreshed"),
            Ok(false) => {}
            Err(e)    => tracing::warn!("github token refresh skipped: {e}"),
        }
    } else if host.contains("gitlab") {
        match oauth_gitlab::try_refresh().await {
            Ok(true)  => tracing::debug!("gitlab access token refreshed"),
            Ok(false) => {}
            Err(e)    => tracing::warn!("gitlab token refresh skipped: {e}"),
        }
    }
}

/// Silent token refresh dispatched by provider name ("github" | "gitlab").
/// Mirrors `maybe_refresh_for_url` but is meant for REST API calls where the
/// remote URL isn't the natural key (MR/PR commands resolve by repo remote
/// and already know which provider they're talking to). Refresh failures are
/// swallowed — the subsequent API call will surface 401 if the token really
/// is dead and the user will be prompted to reconnect.
pub async fn maybe_refresh_for_provider(provider: &str) {
    match provider {
        "github" => {
            match oauth_github::try_refresh().await {
                Ok(true)  => tracing::debug!("github access token refreshed"),
                Ok(false) => {}
                Err(e)    => tracing::warn!("github token refresh skipped: {e}"),
            }
        }
        "gitlab" => {
            match oauth_gitlab::try_refresh().await {
                Ok(true)  => tracing::debug!("gitlab access token refreshed"),
                Ok(false) => {}
                Err(e)    => tracing::warn!("gitlab token refresh skipped: {e}"),
            }
        }
        _ => {}
    }
}

/// Device Authorization Grant response (GitHub / GitLab Device Flow).
///
/// Wire shape returned to the frontend by `start_github_device_flow`. The
/// underlying transport lives in [`arbor_auth::oauth2::DeviceFlow`]; this
/// struct is the public API surface that has shipped to consumers, kept
/// stable for the IPC layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceFlowInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}
