//! Google OAuth 2.0 installed-app flow — thin wrapper around
//! [`arbor_auth::oauth2::InstalledAppFlow`] that handles the
//! Google-specific bits:
//!
//!   * `access_type=offline` + `prompt=consent` so the first authorisation
//!     yields a refresh token even on subsequent grants;
//!   * `BodyFormat::Form` for Google's token endpoint;
//!   * persistence of the [`StoredOAuth`] blob into the cloud-storage
//!     keyring under the plugin-chosen `secret_ref`;
//!   * delivery of the `cloud-storage:oauth-done` plugin hook so the Lua
//!     orchestrator can react to the result.
//!
//! `refresh_with` re-uses [`arbor_auth::oauth2::refresh_token`] and is
//! registered as the [`crate::auth_gcs::OAuthRefresher`] at startup via
//! [`install_refresher`].

use std::sync::Arc;

use arbor_auth::oauth2::{InstalledAppFlow, refresh_token};
use arbor_auth::{BodyFormat, TokenResponse};

use crate::auth_gcs::{now_secs, RefreshedToken, StoredOAuth};
use crate::error::{CloudError, Result};
use crate::host::CloudHost;
use crate::secrets;

const AUTH_URL:        &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL:       &str = "https://oauth2.googleapis.com/token";
pub const CALLBACK_PORT: u16 = 7732;
const SCOPE:           &str = "https://www.googleapis.com/auth/devstorage.read_write";

const PLUGIN_NAME:     &str = "cloud-storage";
const HOOK_OAUTH_DONE: &str = "cloud-storage:oauth-done";
const PROVIDER_LABEL:  &str = "Google Cloud Storage";

/// Start the OAuth flow. Returns the authorization URL the frontend opens
/// in the browser. A background task awaits the loopback callback, persists
/// tokens under `secret_ref`, and fires the `cloud-storage:oauth-done`
/// plugin hook with `{ ok: bool, error?: string }`.
pub async fn start(
    host:          Arc<dyn CloudHost>,
    secret_ref:    String,
    client_id:     String,
    client_secret: Option<String>,
) -> Result<String> {
    if client_id.trim().is_empty() {
        return Err(CloudError::AuthFailed(
            "Google OAuth requires a client_id. Register a Desktop OAuth client at \
             https://console.cloud.google.com/apis/credentials and paste its client_id \
             (and client_secret) into the connection form.".into()
        ));
    }

    let flow = InstalledAppFlow {
        auth_url:               AUTH_URL.into(),
        token_url:              TOKEN_URL.into(),
        client_id:              client_id.clone(),
        client_secret:          client_secret.clone(),
        scope:                  SCOPE.into(),
        redirect_port:          CALLBACK_PORT,
        // `access_type=offline` is what makes Google return a refresh_token
        // the first time; `prompt=consent` forces the consent screen on
        // subsequent grants so a re-auth (after the refresh token was
        // revoked / expired) yields a fresh one instead of silently failing.
        extra_authorize_params: vec![
            ("access_type".into(), "offline".into()),
            ("prompt".into(),      "consent".into()),
        ],
        token_request_format:   BodyFormat::Form,
        provider_label:         PROVIDER_LABEL.into(),
        success_html:           None,
        error_html_template:    None,
    };

    let (auth_url, pending) = flow.start().await
        .map_err(|e| CloudError::Other(format!("oauth start: {e}")))?;

    tokio::spawn(async move {
        let payload = match pending.await_callback().await {
            Ok(token) => persist_and_payload(token, &secret_ref, client_id, client_secret),
            Err(e)    => serde_json::json!({ "ok": false, "error": e.to_string() }),
        };
        let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
        host.fire_plugin_hook(PLUGIN_NAME, HOOK_OAUTH_DONE, &json);
    });

    Ok(auth_url)
}

/// Assemble the keyring blob from the token response and persist it.
/// Returns the hook payload describing the outcome.
fn persist_and_payload(
    token:         TokenResponse,
    secret_ref:    &str,
    client_id:     String,
    client_secret: Option<String>,
) -> serde_json::Value {
    let refresh = token.refresh_token.clone().unwrap_or_default();
    if refresh.is_empty() {
        return serde_json::json!({
            "ok": false,
            "error": "Google did not return a refresh token. \
                      Reset the app's grant at https://myaccount.google.com/permissions \
                      and try again."
        });
    }
    let stored = StoredOAuth {
        refresh_token: refresh,
        access_token:  Some(token.access_token),
        expires_at:    token.expires_in.map(|s| now_secs() + (s.max(0) as u64).saturating_sub(30)),
        client_id:     Some(client_id),
        client_secret,
    };
    let body = match serde_json::to_string(&stored) {
        Ok(s)  => s,
        Err(e) => {
            tracing::error!("cloud oauth: encode tokens: {e}");
            return serde_json::json!({ "ok": false, "error": format!("encode tokens: {e}") });
        }
    };
    if let Err(e) = secrets::set(secret_ref, &body) {
        tracing::error!("cloud oauth: save to keyring: {e}");
        return serde_json::json!({ "ok": false, "error": format!("save tokens to keyring: {e}") });
    }
    serde_json::json!({ "ok": true, "secret_ref": secret_ref })
}

/// Exchange the stored refresh token for a fresh access token.
/// Registered as [`crate::auth_gcs::OAuthRefresher`] via [`install_refresher`].
/// Pure HTTP — no UI / browser involvement.
pub async fn refresh_with(stored: &StoredOAuth) -> Result<RefreshedToken> {
    let client_id = stored.client_id.as_deref().unwrap_or("");
    if client_id.is_empty() {
        return Err(CloudError::AuthFailed(
            "OAuth client_id missing from stored token blob — re-run the authorization flow".into()
        ));
    }
    let token = refresh_token(
        TOKEN_URL,
        client_id,
        stored.client_secret.as_deref(),
        &stored.refresh_token,
        BodyFormat::Form,
    ).await.map_err(|e| CloudError::AuthFailed(format!("Google refresh: {e}")))?;

    Ok(RefreshedToken {
        access_token:    token.access_token,
        expires_in_secs: token.expires_in.unwrap_or(0).max(0) as u64,
    })
}

/// Register [`refresh_with`] as the OAuth refresher used by
/// `auth_gcs::resolve`. Call once at startup before any cloud command
/// runs — idempotent (only the first call wins).
pub fn install_refresher() {
    use crate::auth_gcs::{install_oauth_refresher, OAuthRefresher};
    let refresher: OAuthRefresher = Arc::new(|stored| {
        Box::pin(async move { refresh_with(&stored).await })
    });
    install_oauth_refresher(refresher);
}
