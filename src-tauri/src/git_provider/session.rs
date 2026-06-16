//! Shell-side [`SessionProvider`] adapters for git hosts — the only place that
//! maps an opaque provider `account` onto a real keyring entry and runs the
//! OAuth refresh.
//!
//! A `corvus-git-provider-*` crate holds an `Arc<dyn SessionProvider>` and never
//! sees the keyring; these adapters compose the keyring read (`ci_impl`'s
//! token lookup) with the provider's OAuth refresh (`oauth::*_flow`), returning
//! the `{ base_url, auth_header, web_base }` the backend needs.

use async_trait::async_trait;

use arbor_ipc::prelude::{AuthSession, CredentialError, SessionProvider};

/// GitHub credentials: keyring read + Device-Flow OAuth refresh, as a fixed
/// `Bearer` session against `api.github.com`. Single-account, so `account` is
/// informational.
pub struct GithubSessionProvider;

impl GithubSessionProvider {
    pub fn new() -> Self {
        Self
    }

    fn read() -> Result<AuthSession, CredentialError> {
        match crate::git_provider::ci_impl::get_github_token() {
            Ok(Some(token)) => Ok(AuthSession {
                base_url:    "https://api.github.com".to_string(),
                auth_header: format!("Bearer {token}"),
                web_base:    Some("https://github.com".to_string()),
            }),
            Ok(None) => Err(CredentialError::NotFound("github".into())),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }
}

impl Default for GithubSessionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionProvider for GithubSessionProvider {
    async fn session(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        Self::read()
    }

    async fn refresh(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        // GitHub "Expiring user tokens" use single-use refresh tokens; the flow
        // serializes + coalesces concurrent refreshes internally.
        let refreshed = crate::git_provider::oauth::github_flow::try_refresh()
            .await
            .map_err(|e| CredentialError::Refresh(e.to_string()))?;
        if !refreshed {
            // PAT / non-expiring OAuth tokens have nothing to refresh; the caller
            // surfaces the original 401 as the usual auth error.
            return Err(CredentialError::Refresh("no GitHub refresh token".into()));
        }
        Self::read().map_err(|e| CredentialError::Refresh(e.to_string()))
    }

    fn has_credentials(&self, _account: &str) -> bool {
        crate::git_provider::ci_impl::get_github_token()
            .ok()
            .flatten()
            .is_some()
    }
}

/// GitLab credentials: keyring read + (gitlab.com-only) OAuth refresh, as a
/// `Bearer` session. The `account` IS the instance host root (e.g.
/// `https://gitlab.com` or `https://gitlab.example.org`) — `get_gitlab_token`
/// keys the credential store off it, and refresh applies only to gitlab.com.
pub struct GitlabSessionProvider;

impl GitlabSessionProvider {
    pub fn new() -> Self {
        Self
    }

    fn read(account: &str) -> Result<AuthSession, CredentialError> {
        match crate::git_provider::ci_impl::get_gitlab_token(account) {
            Ok(Some(token)) => Ok(AuthSession {
                base_url:    account.to_string(),
                auth_header: format!("Bearer {token}"),
                web_base:    Some(account.to_string()),
            }),
            Ok(None) => Err(CredentialError::NotFound("gitlab".into())),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }
}

impl Default for GitlabSessionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SessionProvider for GitlabSessionProvider {
    async fn session(&self, account: &str) -> Result<AuthSession, CredentialError> {
        Self::read(account)
    }

    async fn refresh(&self, account: &str) -> Result<AuthSession, CredentialError> {
        // OAuth refresh exists only for gitlab.com; self-hosted uses PATs, which
        // have nothing to refresh (the 401 then propagates as before).
        if !account.contains("gitlab.com") {
            return Err(CredentialError::Refresh("self-hosted GitLab uses a PAT; nothing to refresh".into()));
        }
        let refreshed = crate::git_provider::oauth::gitlab_flow::try_refresh()
            .await
            .map_err(|e| CredentialError::Refresh(e.to_string()))?;
        if !refreshed {
            return Err(CredentialError::Refresh("no GitLab refresh token".into()));
        }
        Self::read(account).map_err(|e| CredentialError::Refresh(e.to_string()))
    }

    fn has_credentials(&self, account: &str) -> bool {
        crate::git_provider::ci_impl::get_gitlab_token(account)
            .ok()
            .flatten()
            .is_some()
    }
}
