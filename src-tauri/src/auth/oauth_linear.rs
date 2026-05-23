//! Linear OAuth 2.0 Authorization Code flow with PKCE.
//!
//! Linear does not support RFC 8628 Device Flow, so we use the standard
//! Authorization Code flow via [`arbor_auth::oauth2::InstalledAppFlow`]:
//! a temporary loopback HTTP server receives the redirect, the crate
//! exchanges the code for tokens, and we persist them in the OS keychain.
//!
//! Flow:
//!   1. `start_linear_oauth(app_handle)` configures `InstalledAppFlow`,
//!      binds the listener and returns the authorize URL.
//!   2. User approves in the browser.
//!   3. Linear redirects to `http://127.0.0.1:7729/callback?code=…`.
//!   4. `arbor-auth` exchanges code → tokens.
//!   5. Access token (and optional refresh token) persisted in the keychain.
//!   6. Emit `arbor://linear-oauth-done` (bool).

use tauri::Emitter;

use arbor_auth::oauth2::{InstalledAppFlow, refresh_token};
use arbor_auth::BodyFormat;

use crate::auth::credential_store;
use crate::error::{AppError, Result};

/// Keychain slot — shared with the PAT flow so the GraphQL client works
/// regardless of which authentication method was used.
const KR_HOST:    &str = "linear.app";
const KR_USER:    &str = "api-key";
const KR_REFRESH: &str = "linear.app-refresh";

const AUTH_URL:  &str = "https://linear.app/oauth/authorize";
const TOKEN_URL: &str = "https://api.linear.app/oauth/token";

/// Space-separated scopes; `arbor-auth` URL-encodes for the authorize URL.
const SCOPE: &str = "read write issues:create comments:create";

/// Bundled Linear OAuth 2.0 application client ID (public client, no secret
/// needed with PKCE).  Users can override this in `~/.config/arbor/config.toml`
/// under `[oauth.linear] client_id = "..."` to point Arbor at their own Linear
/// OAuth application (register at: linear.app → Settings → API).
/// Redirect URI to register: http://127.0.0.1:7729/callback
pub const DEFAULT_CLIENT_ID: &str = "cb88b01e3abce2d3d73fcde29c2c6f0d";

/// Fixed loopback port for the OAuth callback server.
/// Must match the redirect URI registered in the Linear OAuth application.
const CALLBACK_PORT: u16 = 7729;

/// Resolve the active Linear `client_id` — config override or bundled default.
fn resolve_client_id() -> String {
    crate::config::app_config::OAuthOverrides::load_from_disk()
        .linear
        .client_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Start the Linear OAuth 2.0 Authorization Code + PKCE flow.
///
/// Spawns a background task that waits for the browser callback, exchanges
/// the code for a token, persists it in the keychain, and emits
/// `arbor://linear-oauth-done` (payload: `true` on success, `false` on error).
///
/// Returns the authorization URL that the frontend should open in the browser.
pub async fn start_linear_oauth(app_handle: tauri::AppHandle) -> Result<String> {
    let client_id = resolve_client_id();

    let flow = InstalledAppFlow {
        auth_url:               AUTH_URL.into(),
        token_url:              TOKEN_URL.into(),
        client_id,
        client_secret:          None,
        scope:                  SCOPE.into(),
        redirect_port:          CALLBACK_PORT,
        extra_authorize_params: vec![],
        token_request_format:   BodyFormat::Form,
        provider_label:         "Linear".into(),
        success_html:           None,
        error_html_template:    None,
    };

    let (auth_url, pending) = flow.start().await
        .map_err(|e| AppError::Other(format!("Linear OAuth start: {e}")))?;

    let app = app_handle.clone();
    tokio::spawn(async move {
        let ok = match pending.await_callback().await {
            Ok(token) => match persist(&token.access_token, token.refresh_token.as_deref()) {
                Ok(_)  => true,
                Err(e) => {
                    tracing::error!("linear oauth: keychain save failed: {e}");
                    false
                }
            },
            Err(e) => {
                tracing::error!("linear oauth callback error: {e}");
                false
            }
        };
        let _ = app.emit("arbor://linear-oauth-done", ok);
    });

    Ok(auth_url)
}

fn persist(access: &str, refresh: Option<&str>) -> Result<()> {
    credential_store::save(KR_HOST, KR_USER, access)?;
    if let Some(rt) = refresh {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, rt) {
            tracing::warn!("linear oauth: failed to save refresh token: {e}");
        }
    }
    Ok(())
}

/// Attempt to obtain a fresh access token using the stored refresh token.
///
/// Returns `true` when a new access token was obtained and persisted,
/// `false` when no refresh token is stored (PAT flow, or not yet authed via OAuth).
pub async fn try_refresh() -> Result<bool> {
    let Some(refresh) = credential_store::get(KR_REFRESH, KR_USER)? else {
        return Ok(false);
    };

    let client_id = resolve_client_id();
    let token = match refresh_token(TOKEN_URL, &client_id, None, &refresh, BodyFormat::Form).await {
        Ok(t)  => t,
        Err(e) => {
            tracing::warn!("linear token refresh failed: {e}");
            return Ok(false);
        }
    };

    credential_store::save(KR_HOST, KR_USER, &token.access_token)?;
    if let Some(new_rt) = token.refresh_token {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &new_rt) {
            tracing::warn!("linear oauth: failed to update refresh token: {e}");
        }
    }
    tracing::debug!("linear access token refreshed successfully");
    Ok(true)
}

// ── Status / disconnect ───────────────────────────────────────────────────────

/// Returns `true` when a Linear token (PAT or OAuth) is present in the keychain.
pub fn get_status() -> Result<bool> {
    Ok(credential_store::get(KR_HOST, KR_USER)?.is_some())
}

/// Remove the Linear access token and refresh token from the keychain.
pub fn disconnect() -> Result<()> {
    let _ = credential_store::delete(KR_REFRESH, KR_USER); // ignore if absent
    credential_store::delete(KR_HOST, KR_USER)
}
