/// Generic Device Authorization Grant (RFC 8628) helper.
///
/// Both `oauth_github` and `oauth_gitlab` implement the same Device Flow protocol.
/// This module captures the shared logic — HTTP calls, response parsing, keychain
/// persistence — parameterised by provider-specific constants.
///
/// ## Usage
/// ```rust
/// let provider = DeviceFlowProvider {
///     device_code_url: "https://example.com/device/code",
///     token_url:       "https://example.com/token",
///     client_id:       "my-client-id",
///     scope:           "repo",
///     kr_host:         "example.com/arbor",
///     kr_user:         "oauth",
///     /// Some providers (GitHub) return the token body as text/form-encoded
///     /// rather than application/json, so we decode from text. Setting this
///     /// to `false` uses `.json()` directly (GitLab behaviour).
///     token_from_text: true,
///     /// Set to the provider's token endpoint when it issues short-lived tokens
///     /// with a refresh_token (e.g. GitLab). `None` disables the refresh flow.
///     refresh_url: None,
/// };
/// let info = provider.start_device_flow().await?;
/// let result = provider.poll_device_token(info.device_code).await?;
/// ```

use serde::Deserialize;

use crate::auth::{DeviceFlowInfo, PollResult};
use crate::error::{AppError, Result};

pub struct DeviceFlowProvider {
    pub device_code_url: &'static str,
    pub token_url:       &'static str,
    /// Resolved at call time (may be a user-supplied override).
    pub client_id:       String,
    pub scope:           &'static str,
    /// Keychain host key (e.g. `"github.com/arbor"`).
    pub kr_host:         &'static str,
    /// Keychain username key (e.g. `"oauth"`).
    pub kr_user:         &'static str,
    /// When `true`, the token response body is read as text and then
    /// deserialized from JSON (GitHub quirk). When `false`, `.json()` is used
    /// directly (GitLab behaviour).
    pub token_from_text: bool,
    /// Human-readable provider name used in error messages (e.g. `"GitHub"`).
    pub provider_name:   &'static str,
    /// Token endpoint for the OAuth refresh flow (RFC 6749 §6).
    /// `None` means the provider issues non-expiring tokens (e.g. GitHub PAT).
    /// When set, `poll_device_token` persists the `refresh_token` received from
    /// the initial authorisation, and `refresh_access_token` uses it to obtain
    /// a new access token transparently.
    pub refresh_url:     Option<&'static str>,
}

/// Keychain key under which the refresh token is stored: `"{kr_host}-refresh"`.
fn refresh_kr_key(kr_host: &str) -> String {
    format!("{kr_host}-refresh")
}

impl DeviceFlowProvider {
    /// Start the Device Authorization Flow.
    /// Returns device_code, user_code and verification_uri for the frontend to display.
    pub async fn start_device_flow(&self) -> Result<DeviceFlowInfo> {
        let client = reqwest::Client::new();
        let resp = client
            .post(self.device_code_url)
            .header("Accept", "application/json")
            .form(&[("client_id", self.client_id.as_str()), ("scope", self.scope)])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("{} request failed: {e}", self.provider_name)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::AuthFailed(format!("{} {status}: {body}", self.provider_name)));
        }

        resp.json::<DeviceFlowInfo>()
            .await
            .map_err(|e| AppError::Other(format!("{} response parse: {e}", self.provider_name)))
    }

    /// Poll for an access token using the device_code from `start_device_flow`.
    /// Returns a `PollResult` with status `"pending"`, `"authorized"`, `"denied"`, or `"expired"`.
    pub async fn poll_device_token(&self, device_code: String) -> Result<PollResult> {
        let client = reqwest::Client::new();
        let resp = client
            .post(self.token_url)
            .header("Accept", "application/json")
            .form(&[
                ("client_id",   self.client_id.as_str()),
                ("device_code", device_code.as_str()),
                ("grant_type",  "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("{} poll request failed: {e}", self.provider_name)))?;

        #[derive(Deserialize)]
        struct RawToken {
            access_token:  Option<String>,
            refresh_token: Option<String>,
            error:         Option<String>,
            interval:      Option<u64>,
        }

        let raw: RawToken = if self.token_from_text {
            // GitHub returns the token body as text (sometimes form-encoded,
            // sometimes JSON depending on the Accept header). Reading as text
            // then parsing JSON is the most reliable approach.
            let body = resp.text().await
                .map_err(|e| AppError::Other(format!("{} poll read: {e}", self.provider_name)))?;
            serde_json::from_str(&body)
                .map_err(|e| AppError::Other(format!("{} poll parse: {e} — body: {body}", self.provider_name)))?
        } else {
            resp.json::<RawToken>().await
                .map_err(|e| AppError::Other(format!("{} poll parse: {e}", self.provider_name)))?
        };

        if let Some(token) = raw.access_token {
            crate::auth::credential_store::save(self.kr_host, self.kr_user, &token)?;

            // Persist the refresh_token when present (e.g. GitLab).
            if self.refresh_url.is_some() {
                if let Some(rt) = raw.refresh_token {
                    let rk = refresh_kr_key(self.kr_host);
                    if let Err(e) = crate::auth::credential_store::save(&rk, self.kr_user, &rt) {
                        tracing::warn!("failed to save {} refresh token: {e}", self.provider_name);
                    }
                }
            }

            return Ok(PollResult { status: "authorized".into(), token: Some(token), interval: None });
        }

        let status = match raw.error.as_deref() {
            Some("authorization_pending") | Some("slow_down") => "pending",
            Some("expired_token")  => "expired",
            Some("access_denied")  => "denied",
            _                      => "pending",
        };
        Ok(PollResult { status: status.into(), token: None, interval: raw.interval })
    }

    /// Attempt to obtain a fresh access token using the stored refresh token (RFC 6749 §6).
    ///
    /// Returns `true` when a new access token was successfully obtained and stored,
    /// `false` when refresh is not configured or no refresh token is available.
    /// Network or server errors are propagated.
    pub async fn refresh_access_token(&self) -> Result<bool> {
        let Some(refresh_url) = self.refresh_url else {
            return Ok(false);
        };

        let rk = refresh_kr_key(self.kr_host);
        let Some(refresh_token) = crate::auth::credential_store::get(&rk, self.kr_user)? else {
            tracing::debug!("{} no refresh token stored — re-authentication required", self.provider_name);
            return Ok(false);
        };

        let client = reqwest::Client::new();
        let resp = client
            .post(refresh_url)
            .header("Accept", "application/json")
            .form(&[
                ("grant_type",    "refresh_token"),
                ("client_id",     self.client_id.as_str()),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await
            .map_err(|e| AppError::Other(format!("{} token refresh request failed: {e}", self.provider_name)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!("{} token refresh {status}: {body}", self.provider_name);
            return Ok(false);
        }

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token:  String,
            refresh_token: Option<String>,
        }

        let parsed: RefreshResponse = resp.json().await
            .map_err(|e| AppError::Other(format!("{} token refresh parse: {e}", self.provider_name)))?;

        crate::auth::credential_store::save(self.kr_host, self.kr_user, &parsed.access_token)?;
        if let Some(new_rt) = parsed.refresh_token {
            if let Err(e) = crate::auth::credential_store::save(&rk, self.kr_user, &new_rt) {
                tracing::warn!("failed to update {} refresh token: {e}", self.provider_name);
            }
        }

        tracing::debug!("{} access token refreshed successfully", self.provider_name);
        Ok(true)
    }

    /// Returns `true` if an OAuth token for this provider is stored in the OS keychain.
    #[allow(dead_code)]
    pub fn get_status(&self) -> Result<bool> {
        Ok(crate::auth::credential_store::get(self.kr_host, self.kr_user)?.is_some())
    }

    /// Remove the stored OAuth token (and refresh token, if any) from the OS keychain.
    pub fn disconnect(&self) -> Result<()> {
        if self.refresh_url.is_some() {
            let rk = refresh_kr_key(self.kr_host);
            let _ = crate::auth::credential_store::delete(&rk, self.kr_user);
        }
        crate::auth::credential_store::delete(self.kr_host, self.kr_user)
    }
}
