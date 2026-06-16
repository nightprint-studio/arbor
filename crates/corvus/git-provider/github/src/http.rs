//! GitHub HTTP/session seam: resolves the injected `AuthSession`, runs the
//! `401`→refresh→retry round-trip, and maps GitHub error strings onto
//! `ProviderError` (mirrors the shell's old `github_send_with_refresh` +
//! `app_err_to_provider`). The API host stays hardcoded — see `send`.

use std::sync::Arc;

use arbor_ipc::prelude::{AuthSession, CredentialError, SessionProvider};
use corvus_git_provider_api::prelude::ProviderError;

pub struct GithubHttp {
    session: Arc<dyn SessionProvider>,
    account: String,
    http: reqwest::Client,
}

impl GithubHttp {
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

    pub(crate) fn has_credentials(&self) -> bool {
        self.session.has_credentials(&self.account)
    }

    /// Resolve the current session; NotFound -> Unauthenticated.
    pub(crate) async fn session(&self) -> Result<AuthSession, ProviderError> {
        self.session.session(&self.account).await.map_err(|e| match e {
            CredentialError::NotFound(_) => ProviderError::Unauthenticated,
            other => ProviderError::Internal(format!("github token lookup: {other}")),
        })
    }

    /// Send a request built by build(&AuthSession); on HTTP 401 refresh once and retry.
    /// Mirrors the old `github_send_with_refresh`.
    pub(crate) async fn send<F>(&self, build: F) -> Result<reqwest::Response, ProviderError>
    where
        F: Fn(&AuthSession) -> reqwest::RequestBuilder,
    {
        let s = self.session().await?;
        let resp = build(&s)
            .send()
            .await
            .map_err(|e| classify(format!("GitHub API request failed: {e}")))?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Ok(s2) = self.session.refresh(&self.account).await {
                return build(&s2)
                    .send()
                    .await
                    .map_err(|e| classify(format!("GitHub API request failed: {e}")));
            }
            let body = resp.text().await.unwrap_or_default();
            return Err(ProviderError::AuthFailed(format!(
                "GitHub API 401 Unauthorized: {body}"
            )));
        }
        Ok(resp)
    }
}

/// Map a GitHub error message string to a ProviderError, preserving the EXACT
/// classification the shell used before (app_err_to_provider): a message that
/// contains "API 404" becomes NotFound, everything else Internal.
pub(crate) fn classify(msg: String) -> ProviderError {
    if msg.contains("API 404") {
        ProviderError::NotFound(msg)
    } else {
        ProviderError::Internal(msg)
    }
}
