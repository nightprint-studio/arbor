//! GitHub OAuth — Device Authorization Grant (RFC 8628).
//!
//! GitHub OAuth Apps require a `client_secret` for the Authorization Code
//! flow even when PKCE is used, so the secret would have to ship in the
//! Arbor binary.  The Device Authorization Grant requires only the
//! `client_id`, so no secret needs to be embedded in the published source.
//!
//! Flow:
//!   1. Frontend calls `start_github_device_flow` → backend hits
//!      `https://github.com/login/device/code` via [`arbor_auth::oauth2::DeviceFlow`]
//!      and returns `DeviceFlowInfo` (`user_code`, `verification_uri`, …)
//!      for the UI to display.
//!   2. User opens `verification_uri` in their browser and types `user_code`.
//!   3. A spawned task polls `https://github.com/login/oauth/access_token`
//!      (handled by `arbor-auth`) until the user authorises or the code
//!      expires; on success the token is stored in the OS keychain.
//!   4. The Tauri event `arbor://github-oauth-done` is emitted (payload:
//!      `null` on success, error message on failure).
//!
//! # GitHub OAuth application setup
//! Register an application at: github.com → Settings → Developer settings →
//! OAuth Apps.  Enable **Device Flow** in the application settings — no
//! callback URL is needed.

use tauri::Emitter;

use arbor_auth::oauth2::{DeviceFlow, PollOutcome, refresh_token};
use arbor_auth::{AuthError, BodyFormat};

use crate::auth::DeviceFlowInfo;
use crate::auth::credential_store;
use crate::error::{AppError, Result};

const KR_HOST:    &str = "github.com/arbor";
const KR_USER:    &str = "oauth";
const KR_REFRESH: &str = "github.com/arbor-refresh";

/// Bundled GitHub OAuth application client ID.  Users can override this in
/// `~/.config/arbor/config.toml` under `[oauth.github] client_id = "..."`
/// to point Arbor at their own GitHub OAuth App (Device Flow must be enabled
/// for that app).
pub const DEFAULT_CLIENT_ID: &str = "Ov23liYsZ6gFaytjebJY";

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL:       &str = "https://github.com/login/oauth/access_token";

const SCOPE: &str = "repo workflow";

/// Resolve the active GitHub `client_id` — config override or bundled default.
fn resolve_client_id() -> String {
    crate::config::app_config::OAuthOverrides::load_from_disk()
        .github
        .client_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

fn make_flow() -> DeviceFlow {
    DeviceFlow {
        device_code_url: DEVICE_CODE_URL.into(),
        token_url:       TOKEN_URL.into(),
        client_id:       resolve_client_id(),
        scope:           SCOPE.into(),
        format:          BodyFormat::Form,
    }
}

/// Start the GitHub Device Authorization Grant flow.
///
/// Returns the user-facing verification info (`user_code`, `verification_uri`,
/// `expires_in`, `interval`) for the UI to display.  A background task polls
/// the token endpoint until the user completes (or rejects) authorisation,
/// then emits `arbor://github-oauth-done`:
///   - payload `null`        → success, token persisted in the keychain
///   - payload `<string>`    → human-readable error message
pub async fn start_github_device_flow(app_handle: tauri::AppHandle) -> Result<DeviceFlowInfo> {
    let flow = make_flow();
    let code = flow.request_code().await
        .map_err(|e| AppError::Other(format!("GitHub device-code request: {e}")))?;

    let info = DeviceFlowInfo {
        device_code:      code.device_code.clone(),
        user_code:        code.user_code.clone(),
        verification_uri: code.verification_uri.clone(),
        expires_in:       code.expires_in,
        interval:         code.interval,
    };

    let app = app_handle.clone();
    tokio::spawn(async move {
        let result: Option<String> = match flow.poll_until_done(&code).await {
            Ok(PollOutcome::Granted(token)) => match persist(&token.access_token, token.refresh_token.as_deref()) {
                Ok(_)  => None,
                Err(e) => {
                    tracing::error!("github oauth: keychain save failed: {e}");
                    Some(format!("Keychain save failed: {e}"))
                }
            },
            Ok(PollOutcome::AccessDenied)     => Some("Authorization denied.".into()),
            Err(AuthError::DeviceCodeExpired) => Some("Device code expired — please try again.".into()),
            Err(e) => {
                tracing::error!("github device flow poll error: {e}");
                Some(e.to_string())
            }
        };
        let _ = app.emit("arbor://github-oauth-done", result);
    });

    Ok(info)
}

fn persist(access: &str, refresh: Option<&str>) -> Result<()> {
    credential_store::save(KR_HOST, KR_USER, access)?;
    if let Some(rt) = refresh {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, rt) {
            tracing::warn!("github oauth: failed to save refresh token: {e}");
        }
    }
    Ok(())
}

/// Serialize OAuth refresh attempts: GitHub's "Expiring user tokens" feature
/// uses single-use refresh tokens (same as GitLab), so concurrent calls to
/// the token endpoint with the same refresh token would race — only the
/// first succeeds, the others see "invalid refresh token" and would surface
/// as spurious 401s. The lock makes parallel refresh requests coalesce: the
/// first refreshes, the rest re-read the freshly-stored token and
/// short-circuit without calling the token endpoint again.
pub(crate) static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Attempt to obtain a fresh access token using the stored refresh token.
///
/// Always serialized via [`REFRESH_LOCK`]; safe to call from any number of
/// concurrent tasks. Returns `true` when a fresh access token is available in
/// the keychain (either freshly fetched here or rotated by another caller
/// while we waited on the lock). Returns `false` silently when no refresh
/// token is stored — the normal case for GitHub OAuth Apps without
/// "Expiring user tokens" enabled.
pub async fn try_refresh() -> Result<bool> {
    try_refresh_if_stale(None).await
}

/// Refresh the access token, but skip the network round-trip if another
/// caller already rotated it while we were waiting on the lock.
///
/// See `gitlab_flow::try_refresh_if_stale` for the rationale.
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

    let client_id = resolve_client_id();
    let token = match refresh_token(TOKEN_URL, &client_id, None, &refresh, BodyFormat::Form).await {
        Ok(t)  => t,
        Err(e) => {
            tracing::warn!("github token refresh failed: {e}");
            return Ok(false);
        }
    };

    credential_store::save(KR_HOST, KR_USER, &token.access_token)?;
    if let Some(new_rt) = token.refresh_token {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &new_rt) {
            tracing::warn!("github oauth: failed to update refresh token: {e}");
        }
    }

    tracing::debug!("github access token refreshed successfully");
    Ok(true)
}

/// Returns `true` when a GitHub token (PAT or OAuth) is present in the OS keychain.
#[allow(dead_code)]
pub fn get_status() -> Result<bool> {
    Ok(credential_store::get(KR_HOST, KR_USER)?.is_some())
}

/// Remove the GitHub access token (and refresh token, if any) from the OS keychain.
pub fn disconnect() -> Result<()> {
    let _ = credential_store::delete(KR_REFRESH, KR_USER);
    credential_store::delete(KR_HOST, KR_USER)
}
