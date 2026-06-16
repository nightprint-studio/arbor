//! GitLab HTTP/session seam: resolves the injected `AuthSession`, runs the
//! `401`→refresh→retry round-trip, and maps GitLab error strings onto
//! `ProviderError` (mirrors the shell's old `gitlab_send_with_refresh` +
//! `app_err_to_provider`). For GitLab the `account` IS the instance base URL,
//! exposed bare (no `/api/v4`) via [`GitlabHttp::base`] — callers append the
//! `/api/v4/...` or `/api/graphql` suffix themselves.

use std::sync::Arc;

use arbor_ipc::prelude::{AuthSession, CredentialError, SessionProvider};
use corvus_git_provider_api::prelude::ProviderError;

pub struct GitlabHttp {
    session: Arc<dyn SessionProvider>,
    account: String,
    http: reqwest::Client,
}

impl GitlabHttp {
    pub(crate) fn new(session: Arc<dyn SessionProvider>, account: impl Into<String>) -> Self {
        Self {
            session,
            account: account.into(),
            http: reqwest::Client::new(),
        }
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.http
    }

    /// Bare GitLab host root (no /api/v4) — callers append "/api/v4/..." or "/api/graphql".
    pub(crate) fn base(&self) -> &str {
        &self.account
    }

    pub(crate) fn has_credentials(&self) -> bool {
        self.session.has_credentials(&self.account)
    }

    /// Resolve the current session; NotFound -> Unauthenticated.
    pub(crate) async fn session(&self) -> Result<AuthSession, ProviderError> {
        self.session.session(&self.account).await.map_err(|e| match e {
            CredentialError::NotFound(_) => ProviderError::Unauthenticated,
            other => ProviderError::Internal(format!("gitlab token lookup: {other}")),
        })
    }

    /// Send a request built by build(&AuthSession); on HTTP 401 refresh once and retry.
    /// Mirrors the old `gitlab_send_with_refresh`. Self-hosted PAT has no refresh
    /// (session.refresh errors) so the 401 propagates — same as before.
    pub(crate) async fn send<F>(&self, build: F) -> Result<reqwest::Response, ProviderError>
    where
        F: Fn(&AuthSession) -> reqwest::RequestBuilder,
    {
        let s = self.session().await?;
        let resp = build(&s)
            .send()
            .await
            .map_err(|e| classify(format!("GitLab API request failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Ok(s2) = self.session.refresh(&self.account).await {
                return build(&s2)
                    .send()
                    .await
                    .map_err(|e| classify(format!("GitLab API request failed: {e}")));
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::AuthFailed(format!(
                "GitLab API 401 Unauthorized: {body}"
            )));
        }
        Ok(resp)
    }
}

/// Map a GitLab error message string to a ProviderError, preserving the EXACT
/// classification the shell used before (app_err_to_provider): a message that
/// contains "API 404" becomes NotFound, everything else Internal. (GitLab 404
/// messages are "GitLab API 404 ...", which contain "API 404".)
pub(crate) fn classify(msg: String) -> ProviderError {
    if msg.contains("API 404") {
        ProviderError::NotFound(msg)
    } else {
        ProviderError::Internal(msg)
    }
}

/// Percent-encode '/' as %2F for GitLab project paths in URL segments.
pub(crate) fn percent_encode_slash(s: &str) -> String {
    s.replace('/', "%2F")
}
