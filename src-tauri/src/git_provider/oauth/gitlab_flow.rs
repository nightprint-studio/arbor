//! GitLab OAuth 2.0 Authorization Code flow with PKCE.
//!
//! GitLab supports RFC 7636 PKCE since version 12.3. We use the standard
//! Authorization Code flow via [`arbor_auth::oauth2::InstalledAppFlow`]: a
//! temporary loopback HTTP server receives the redirect, the crate exchanges
//! the code for tokens, and we persist them in the OS keychain.
//!
//! Flow:
//!   1. `start_gitlab_oauth(app_handle)` configures `InstalledAppFlow` against
//!      the resolved `base_host` (gitlab.com by default, or a self-hosted
//!      override), binds the loopback listener on port 7731 and returns the
//!      authorize URL.
//!   2. Frontend opens the URL → user authenticates in the browser.
//!   3. GitLab redirects to `http://127.0.0.1:7731/callback?code=…&state=…`.
//!   4. `arbor-auth` exchanges the code for `(access, refresh)` tokens.
//!   5. Tokens are persisted in two keychain slots; the Tauri event
//!      `arbor://gitlab-oauth-done` (`Option<String>`: `null` on success,
//!      error message on failure) is emitted.
//!
//! # GitLab OAuth application setup
//! Register an application at: gitlab.com → Preferences → Applications
//! (or Admin → Applications for instance-wide). Settings:
//!   - Redirect URI: `http://127.0.0.1:7731/callback`
//!   - Scopes: `api`
//!   - Confidential: No (public client — PKCE is used instead of a secret)

use tauri::Emitter;

use arbor_auth::oauth2::{InstalledAppFlow, refresh_token};
use arbor_auth::BodyFormat;

use crate::auth::credential_store;
use crate::error::{AppError, Result};

/// Keychain slot — shared with the PAT flow so the CI client, remote commands,
/// and any other GitLab API callers work regardless of how the user authenticated.
const KR_HOST:    &str = "gitlab.com/arbor";
const KR_USER:    &str = "oauth";
const KR_REFRESH: &str = "gitlab.com/arbor-refresh";

/// Bundled GitLab OAuth 2.0 application client ID (public client, no secret
/// needed with PKCE).  Users can override this in `~/.config/arbor/config.toml`
/// under `[oauth.gitlab] client_id = "..."`, and switch the base host to point
/// at a self-hosted instance via `base_host = "gitlab.example.com"`.
/// Ensure the redirect URI `http://127.0.0.1:7731/callback` is registered for
/// the OAuth app.
pub const DEFAULT_CLIENT_ID: &str = "a8919bfd56e55d1085e7ad6c05f45511f7af1fb6d80d853cc874c3094747b8e3";

/// Default GitLab base host.  Self-hosted instances override via config.
pub const DEFAULT_BASE_HOST: &str = "gitlab.com";

/// Fixed loopback port for the OAuth callback server.
/// Must match the redirect URI registered in the GitLab OAuth application.
const CALLBACK_PORT: u16 = 7731;

/// Read the active GitLab OAuth overrides from disk, falling back to defaults.
/// Returns `(client_id, base_host)`.
fn resolve_overrides() -> (String, String) {
    let o = crate::config::app_config::OAuthOverrides::load_from_disk().gitlab;
    let client_id = o.client_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());
    let base_host = o.base_host
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_HOST.to_string());
    (client_id, base_host)
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Start the GitLab OAuth 2.0 Authorization Code + PKCE flow.
///
/// Spawns a background task that waits for the browser callback, exchanges
/// the code for a token, persists it (plus the refresh token) in the keychain,
/// and emits `arbor://gitlab-oauth-done` (payload: `null` on success,
/// error string on failure).
///
/// Returns the authorization URL that the frontend should open in the default browser.
pub async fn start_gitlab_oauth(app_handle: tauri::AppHandle) -> Result<String> {
    let (client_id, base_host) = resolve_overrides();

    let flow = InstalledAppFlow {
        auth_url:               format!("https://{base_host}/oauth/authorize"),
        token_url:              format!("https://{base_host}/oauth/token"),
        client_id,
        client_secret:          None,
        scope:                  "api".into(),
        redirect_port:          CALLBACK_PORT,
        extra_authorize_params: vec![],
        token_request_format:   BodyFormat::Form,
        provider_label:         "GitLab".into(),
        success_html:           None,
        error_html_template:    None,
    };

    let (auth_url, pending) = flow.start().await
        .map_err(|e| AppError::Other(format!("GitLab OAuth start: {e}")))?;

    let app = app_handle.clone();
    tokio::spawn(async move {
        let result: Option<String> = match pending.await_callback().await {
            Ok(token) => match persist(&token.access_token, token.refresh_token.as_deref()) {
                Ok(_)  => None,
                Err(e) => {
                    tracing::error!("gitlab oauth: keychain save failed: {e}");
                    Some(format!("Keychain save failed: {e}"))
                }
            },
            Err(e) => {
                tracing::error!("gitlab oauth callback error: {e}");
                Some(e.to_string())
            }
        };
        let _ = app.emit("arbor://gitlab-oauth-done", result);
    });

    Ok(auth_url)
}

fn persist(access: &str, refresh: Option<&str>) -> Result<()> {
    credential_store::save(KR_HOST, KR_USER, access)?;
    if let Some(rt) = refresh {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, rt) {
            tracing::warn!("gitlab oauth: failed to save refresh token: {e}");
        }
    }
    Ok(())
}

/// Serialize OAuth refresh attempts: GitLab's refresh token is single-use
/// (rotating), so concurrent calls to `/oauth/token` with the same refresh
/// token would race — only the first succeeds, the others see "invalid
/// refresh token" and would surface as spurious 401s. The lock makes
/// parallel refresh requests coalesce: the first refreshes, the rest
/// re-read the freshly-stored token and short-circuit without calling
/// `/oauth/token` again.
pub(crate) static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Attempt to obtain a fresh access token using the stored refresh token (RFC 6749 §6).
///
/// Always serialized via [`REFRESH_LOCK`]; safe to call from any number of
/// concurrent tasks. Returns `true` when a new access token is available in
/// the keychain (either freshly fetched here or rotated by another caller
/// while we waited on the lock); `false` when no refresh token is stored
/// (PAT auth flow, or not yet authed via OAuth).
pub async fn try_refresh() -> Result<bool> {
    try_refresh_if_stale(None).await
}

/// Refresh the access token, but skip the network round-trip if another
/// caller already rotated it while we were waiting on the lock.
///
/// `stale_access_token` is the access token the caller used in the request
/// that came back with 401. After acquiring the lock we re-read the keychain
/// and, if the access token has changed, we just signal success — the caller
/// can then retry with whatever's now stored, no `/oauth/token` call needed
/// (which would fail anyway since the previous refresh token has been used).
///
/// Pass `None` to force an unconditional refresh (still serialized).
pub async fn try_refresh_if_stale(stale_access_token: Option<&str>) -> Result<bool> {
    let _guard = REFRESH_LOCK.lock().await;

    if let Some(stale) = stale_access_token {
        let current = credential_store::get(KR_HOST, KR_USER)?;
        if matches!(&current, Some(t) if t != stale) {
            // Another caller refreshed while we were queued — keychain has a
            // fresh access token. Skip the round-trip.
            return Ok(true);
        }
    }

    let Some(refresh) = credential_store::get(KR_REFRESH, KR_USER)? else {
        return Ok(false);
    };

    let (client_id, base_host) = resolve_overrides();
    let token_url = format!("https://{base_host}/oauth/token");
    let token = match refresh_token(&token_url, &client_id, None, &refresh, BodyFormat::Form).await {
        Ok(t)  => t,
        Err(e) => {
            tracing::warn!("gitlab token refresh failed: {e}");
            return Ok(false);
        }
    };

    credential_store::save(KR_HOST, KR_USER, &token.access_token)?;
    if let Some(new_rt) = token.refresh_token {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &new_rt) {
            tracing::warn!("gitlab oauth: failed to update refresh token: {e}");
        }
    }

    tracing::debug!("gitlab access token refreshed successfully");
    Ok(true)
}

/// Returns `true` when a GitLab token (PAT or OAuth) is present in the OS keychain.
#[allow(dead_code)]
pub fn get_status() -> Result<bool> {
    Ok(credential_store::get(KR_HOST, KR_USER)?.is_some())
}

/// Remove the GitLab access token and refresh token from the OS keychain.
pub fn disconnect() -> Result<()> {
    let _ = credential_store::delete(KR_REFRESH, KR_USER);
    credential_store::delete(KR_HOST, KR_USER)
}
