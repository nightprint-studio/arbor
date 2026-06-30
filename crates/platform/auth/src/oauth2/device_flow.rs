//! OAuth 2.0 Device Authorization Grant (RFC 8628).
//!
//! Typical usage (GitHub-style):
//!
//! ```ignore
//! let flow = DeviceFlow {
//!     device_code_url: "https://github.com/login/device/code".into(),
//!     token_url:       "https://github.com/login/oauth/access_token".into(),
//!     client_id:       "Iv1.xxxxxxxx".into(),
//!     scope:           "repo read:org".into(),
//!     format:          BodyFormat::Form,
//! };
//! let code = flow.request_code().await?;
//! // Show `code.user_code` + `code.verification_uri` to the user.
//! match flow.poll_until_done(&code).await? {
//!     PollOutcome::Granted(token) => { /* persist + use */ },
//!     PollOutcome::AccessDenied   => { /* user declined */ },
//! }
//! ```
//!
//! ⚠ **Untested against a real provider yet.** The shape follows the RFC
//! literally; the first real consumer (likely a GitHub PAT-style flow in
//! `arbor-git-provider-github`) may need small adjustments — for example
//! GitHub returns the form-encoded body even when `Accept: application/json`
//! is set unless you pass `Accept: application/json` explicitly. Refine
//! when the first consumer lands.

use std::time::{Duration, Instant};

use crate::error::{AuthError, Result};
use crate::types::{BodyFormat, DeviceCode, TokenResponse, TokenWire};

pub struct DeviceFlow {
    pub device_code_url: String,
    pub token_url:       String,
    pub client_id:       String,
    pub scope:           String,
    pub format:          BodyFormat,
}

/// Result of a successful poll attempt.
#[derive(Debug)]
pub enum PollOutcome {
    /// User authorized — tokens are ready.
    Granted(TokenResponse),
    /// User explicitly declined the authorization request.
    AccessDenied,
}

impl DeviceFlow {
    /// Hit the device-code endpoint and decode the response into a
    /// [`DeviceCode`]. The caller is responsible for displaying
    /// `user_code` + `verification_uri` to the user.
    pub async fn request_code(&self) -> Result<DeviceCode> {
        let client = reqwest::Client::new();
        let req = client.post(&self.device_code_url)
            .header("Accept", "application/json");
        let req = match self.format {
            BodyFormat::Form => req.form(&[
                ("client_id", self.client_id.as_str()),
                ("scope",     self.scope.as_str()),
            ]),
            BodyFormat::Json => req.json(&serde_json::json!({
                "client_id": self.client_id,
                "scope":     self.scope,
            })),
        };
        let resp = req.send().await
            .map_err(|e| AuthError::Http(format!("device_code request: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::HttpStatus { status: status.as_u16(), body });
        }
        let code: DeviceCode = resp.json().await
            .map_err(|e| AuthError::Http(format!("device_code decode: {e}")))?;
        Ok(code)
    }

    /// Poll the token endpoint at the provider-specified `interval`,
    /// honouring `authorization_pending` (keep polling), `slow_down`
    /// (bump the interval by 5s as RFC 8628 §3.5 dictates) and
    /// `expired_token` (terminal — returns [`AuthError::DeviceCodeExpired`]).
    ///
    /// Bounded by `code.expires_in` so a misconfigured provider can't
    /// trap us in an infinite loop.
    pub async fn poll_until_done(&self, code: &DeviceCode) -> Result<PollOutcome> {
        let deadline = Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = Duration::from_secs(code.interval);

        loop {
            tokio::time::sleep(interval).await;
            if Instant::now() >= deadline {
                return Err(AuthError::DeviceCodeExpired);
            }

            let client = reqwest::Client::new();
            let req = client.post(&self.token_url)
                .header("Accept", "application/json");
            let req = match self.format {
                BodyFormat::Form => req.form(&[
                    ("client_id",   self.client_id.as_str()),
                    ("device_code", code.device_code.as_str()),
                    ("grant_type",  "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
                BodyFormat::Json => req.json(&serde_json::json!({
                    "client_id":   self.client_id,
                    "device_code": code.device_code,
                    "grant_type":  "urn:ietf:params:oauth:grant-type:device_code",
                })),
            };
            let resp = req.send().await
                .map_err(|e| AuthError::Http(format!("device-poll request: {e}")))?;

            // Per RFC 8628 §3.5, pending / slow_down come back as 400 with
            // an `error` field in the JSON body. So we always parse the
            // body, then branch on `error` before treating !success as fatal.
            let status = resp.status();
            let raw: serde_json::Value = resp.json().await
                .map_err(|e| AuthError::Http(format!("device-poll decode: {e}")))?;

            if let Some(err) = raw.get("error").and_then(|v| v.as_str()) {
                match err {
                    "authorization_pending" => continue,
                    "slow_down" => {
                        interval += Duration::from_secs(5);
                        continue;
                    }
                    "expired_token" => return Err(AuthError::DeviceCodeExpired),
                    "access_denied" => return Ok(PollOutcome::AccessDenied),
                    other => return Err(AuthError::Provider(format!(
                        "device-flow polling failed: {other}"
                    ))),
                }
            }

            if !status.is_success() {
                let body = serde_json::to_string(&raw).unwrap_or_default();
                return Err(AuthError::HttpStatus { status: status.as_u16(), body });
            }

            let wire: TokenWire = serde_json::from_value(raw.clone())
                .map_err(|e| AuthError::Http(format!("device-poll token shape: {e}")))?;
            return Ok(PollOutcome::Granted(TokenResponse {
                access_token:  wire.access_token,
                refresh_token: wire.refresh_token,
                expires_in:    wire.expires_in,
                raw,
            }));
        }
    }
}
