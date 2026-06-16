//! Shell-side [`SessionProvider`] adapters — the only place that maps an opaque
//! backend `account` path onto a real keyring entry and runs the OAuth refresh.
//!
//! A `corvus-issue-tracker-*` crate holds an `Arc<dyn SessionProvider>` and
//! never sees the keyring; these adapters compose the keyring read
//! (`credential_store` / `oauth_*`) with the provider's refresh flow, returning
//! the `{ base_url, auth_header }` the backend needs.

use async_trait::async_trait;

use arbor_ipc::prelude::{AuthSession, CredentialError, SessionProvider};
use corvus_issue_tracker_linear::LINEAR_GQL;

use crate::auth::{credential_store, oauth_jira, oauth_linear};

const LINEAR_KR_HOST: &str = "linear.app";
const LINEAR_KR_USER: &str = "api-key";

/// Linear credentials: keyring read + OAuth refresh, as a fixed-endpoint
/// `Bearer` session. Single-account, so `account` is informational.
pub struct LinearSessionProvider;

impl LinearSessionProvider {
    fn read() -> Result<AuthSession, CredentialError> {
        match credential_store::get(LINEAR_KR_HOST, LINEAR_KR_USER) {
            Ok(Some(token)) => Ok(AuthSession {
                base_url: LINEAR_GQL.to_string(),
                auth_header: format!("Bearer {token}"),
                web_base: None, // Linear's API returns issue URLs directly.
            }),
            Ok(None) => Err(CredentialError::NotFound("linear".into())),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }
}

#[async_trait]
impl SessionProvider for LinearSessionProvider {
    async fn session(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        Self::read()
    }

    async fn refresh(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        let refreshed = oauth_linear::try_refresh()
            .await
            .map_err(|e| CredentialError::Refresh(e.to_string()))?;
        if !refreshed {
            // PAT users have nothing to refresh; the caller surfaces this as the
            // usual "expired" message.
            return Err(CredentialError::Refresh("no Linear refresh token".into()));
        }
        Self::read().map_err(|e| CredentialError::Refresh(e.to_string()))
    }
}

/// Jira credentials: per-tenant base URL + auth header (`Bearer`/`Basic`) +
/// web base, from `oauth_jira`. OAuth refresh; API-token auth has none.
pub struct JiraSessionProvider;

impl JiraSessionProvider {
    fn read() -> Result<AuthSession, CredentialError> {
        match oauth_jira::get_config() {
            Ok(Some(cfg)) => Ok(AuthSession {
                base_url:    cfg.base_url,
                auth_header: cfg.auth_header,
                web_base:    cfg.domain,
            }),
            Ok(None) => Err(CredentialError::NotFound("jira".into())),
            Err(e) => Err(CredentialError::Store(e.to_string())),
        }
    }
}

#[async_trait]
impl SessionProvider for JiraSessionProvider {
    async fn session(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        Self::read()
    }

    async fn refresh(&self, _account: &str) -> Result<AuthSession, CredentialError> {
        let refreshed = oauth_jira::try_refresh()
            .await
            .map_err(|e| CredentialError::Refresh(e.to_string()))?;
        if !refreshed {
            return Err(CredentialError::Refresh("no Jira OAuth refresh token".into()));
        }
        Self::read().map_err(|e| CredentialError::Refresh(e.to_string()))
    }
}
