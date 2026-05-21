//! GitHub OAuth — Device Authorization Grant (RFC 8628).
//!
//! GitHub OAuth Apps require a `client_secret` for the Authorization Code
//! flow even when PKCE is used, so the secret would have to ship in the
//! Arbor binary.  The Device Authorization Grant requires only the
//! `client_id`, so no secret needs to be embedded in the published source.
//!
//! Flow:
//!   1. Frontend calls `start_github_device_flow` → backend POSTs to
//!      `https://github.com/login/device/code`.  Returns `DeviceFlowInfo`
//!      (`user_code`, `verification_uri`, …) for the UI to display, and
//!      spawns a background polling task.
//!   2. User opens `verification_uri` in their browser and types `user_code`.
//!   3. The polling task POSTs to `https://github.com/login/oauth/access_token`
//!      with `grant_type=urn:ietf:params:oauth:grant-type:device_code` until
//!      authorization completes.  The token is stored in the OS keychain.
//!   4. The Tauri event `arbor://github-oauth-done` is emitted (payload:
//!      `null` on success, error message on failure).
//!
//! # GitHub OAuth application setup
//! Register an application at: github.com → Settings → Developer settings →
//! OAuth Apps.  Enable **Device Flow** in the application settings — no
//! callback URL is needed.

use tauri::Emitter;

use crate::auth::DeviceFlowInfo;
use crate::auth::device_flow::DeviceFlowProvider;
use crate::error::Result;

const KR_HOST:   &str = "github.com/arbor";
const KR_USER:   &str = "oauth";

/// Bundled GitHub OAuth application client ID.  Users can override this in
/// `~/.config/arbor/config.toml` under `[oauth.github] client_id = "..."`
/// to point Arbor at their own GitHub OAuth App (Device Flow must be enabled
/// for that app).
pub const DEFAULT_CLIENT_ID: &str = "Ov23liYsZ6gFaytjebJY";

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL:       &str = "https://github.com/login/oauth/access_token";

/// Resolve the active GitHub `client_id` — config override or bundled default.
fn resolve_client_id() -> String {
    crate::config::app_config::OAuthOverrides::load_from_disk()
        .github
        .client_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

fn provider() -> DeviceFlowProvider {
    DeviceFlowProvider {
        device_code_url: DEVICE_CODE_URL,
        token_url:       TOKEN_URL,
        client_id:       resolve_client_id(),
        scope:           "repo workflow",
        kr_host:         KR_HOST,
        kr_user:         KR_USER,
        // GitHub returns the token body as text — sometimes JSON, sometimes
        // form-encoded depending on the Accept header.  Reading as text and
        // parsing JSON is the most reliable approach.
        token_from_text: true,
        provider_name:   "GitHub",
        // GitHub returns a `refresh_token` only when the OAuth App owner
        // opts into "Expiring user tokens".  When no refresh token is
        // stored, `refresh_access_token` returns `Ok(false)` silently.
        refresh_url:     Some(TOKEN_URL),
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
    let info = provider().start_device_flow().await?;

    let device_code = info.device_code.clone();
    let interval    = info.interval;
    let expires_in  = info.expires_in;

    tokio::spawn(async move {
        let result = poll_until_done(device_code, interval, expires_in).await;
        let _ = app_handle.emit("arbor://github-oauth-done", result);
    });

    Ok(info)
}

/// Poll the token endpoint until authorisation succeeds, fails, or the device
/// code expires.  Returns `None` on success, `Some(error)` otherwise — matching
/// the `Option<String>` payload shape the frontend already listens for.
async fn poll_until_done(
    device_code: String,
    initial_interval: u64,
    expires_in: u64,
) -> Option<String> {
    let p = provider();
    let start = std::time::Instant::now();
    let mut interval_secs = initial_interval.max(1);

    loop {
        if start.elapsed().as_secs() >= expires_in {
            return Some("Device code expired — please try again.".into());
        }
        tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;

        match p.poll_device_token(device_code.clone()).await {
            Ok(r) => match r.status.as_str() {
                "authorized" => return None,
                "denied"     => return Some("Authorization denied.".into()),
                "expired"    => return Some("Device code expired — please try again.".into()),
                "pending"    => {
                    // GitHub issues `slow_down` with a new minimum interval; honour it.
                    if let Some(new_iv) = r.interval {
                        interval_secs = new_iv.max(interval_secs.saturating_add(1));
                    }
                    continue;
                }
                other => return Some(format!("Unexpected status: {other}")),
            },
            Err(e) => {
                tracing::error!("github device flow poll error: {e}");
                return Some(e.to_string());
            }
        }
    }
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
        let current = crate::auth::credential_store::get(KR_HOST, KR_USER)?;
        if matches!(&current, Some(t) if t != stale) {
            // Another caller refreshed while we were queued — keychain has a
            // fresh access token. Skip the round-trip.
            return Ok(true);
        }
    }

    provider().refresh_access_token().await
}

/// Returns `true` when a GitHub token (PAT or OAuth) is present in the OS keychain.
#[allow(dead_code)]
pub fn get_status() -> Result<bool> {
    provider().get_status()
}

/// Remove the GitHub access token (and refresh token, if any) from the OS keychain.
pub fn disconnect() -> Result<()> {
    provider().disconnect()
}
