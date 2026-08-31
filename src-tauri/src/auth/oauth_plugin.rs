//! The OAuth engine a plugin drives, with no provider knowledge in it.
//!
//! ## Why this is the host's job and the provider's is not
//!
//! Two halves of an installed-app flow cannot live in a plugin. One is the **loopback
//! catcher**: the browser comes back to `http://127.0.0.1:<port>/`, and listening on a socket
//! is not something a Lua plugin or a wasm guest can do — nor should it be, since that socket
//! is where the authorization code arrives. The other is the **credential store**: the tokens
//! belong in the OS keychain under the plugin's own namespace, which is Arbor's broker to
//! write.
//!
//! Everything else — which endpoints, which scopes, which client, which extra parameters a
//! particular provider insists on — is the plugin's, and arrives here as **data**. So Google's
//! `access_type=offline` + `prompt=consent`, its endpoints and its scope are not in this file
//! and never will be: they are three lines of a Lua table in whichever package speaks Google.
//!
//! ## What lands in the slot
//!
//! One JSON document, in the plugin's own credential slot:
//!
//! ```json
//! { "refresh_token": "…", "access_token": "…", "expires_at": 1730000000,
//!   "client_id": "…", "client_secret": "…" }
//! ```
//!
//! `client_id` / `client_secret` are stored WITH the tokens on purpose: a refresh months later
//! must not depend on a setting the user may have edited since, and a refresh token is only
//! ever redeemable by the client it was issued to.
//!
//! A guest reading the slot (`arbor:host/secrets`) finds `access_token` there — which is how a
//! provider extension authenticates without ever seeing the flow.

use std::time::{SystemTime, UNIX_EPOCH};

use arbor_auth::prelude::{BodyFormat, InstalledAppFlow, TokenResponse, refresh_token};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

/// What a plugin says to start a flow. Every provider-specific decision is one of these
/// fields, which is the whole point.
#[derive(Debug, Clone, Deserialize)]
pub struct StartSpec {
    /// The credential slot the tokens land in. Already checked against the plugin's declared
    /// slots by `arbor.oauth`, in the host that has the plugin's manifest.
    pub slot: String,
    pub auth_url: String,
    pub token_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    /// Space-separated, as the spec writes it. A Lua caller passing a list gets it joined
    /// before it reaches here.
    #[serde(default)]
    pub scope: String,
    /// Loopback port the redirect URI names. Fixed rather than ephemeral because providers
    /// require the redirect URI to be registered up front.
    pub redirect_port: u16,
    /// Extra `?key=value` pairs on the authorize URL — the provider-specific dialect
    /// (`access_type`, `prompt`, `audience`, …).
    #[serde(default)]
    pub extra_params: Vec<(String, String)>,
    /// Whether the token request is form-encoded (most) or JSON (some).
    #[serde(default)]
    pub json_token_request: bool,
    /// Shown on the browser page the user lands on when it is over.
    #[serde(default)]
    pub label: String,
    /// Open the authorize URL in the user's browser. On by default, because a flow that hands
    /// back a URL nobody opens has not started — and a plugin has no way to open one itself.
    /// Turn it off to show the URL instead (a headless run, a machine with no browser).
    #[serde(default = "yes")]
    pub open_browser: bool,
    /// Plugin hook fired with `{ ok, error? }` when the flow finishes. Without one the plugin
    /// has no way to learn the outcome: `start` returns as soon as there is a URL to open.
    #[serde(default)]
    pub on_done: Option<String>,
    /// Refuse to finish without a refresh token. The default, because a flow whose whole
    /// purpose is long-lived access silently succeeding without one is the failure that shows
    /// up an hour later as an expired token and no way back.
    #[serde(default = "yes")]
    pub require_refresh_token: bool,
}

/// What a plugin says to refresh. The endpoint is named again rather than stored, so a
/// provider that moves it does not strand every credential issued before the move.
#[derive(Debug, Clone, Deserialize)]
pub struct RefreshSpec {
    pub slot: String,
    pub token_url: String,
    #[serde(default)]
    pub json_token_request: bool,
    /// Refresh only when the stored access token expires within this many seconds. `0` always
    /// refreshes. The caller that asks before every request wants this; the one reacting to a
    /// 401 does not.
    #[serde(default)]
    pub min_remaining_secs: u64,
}

/// The document in the credential slot. Public because it is a contract: an extension reads
/// this shape out of `arbor:host/secrets`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredTokens {
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub access_token: Option<String>,
    /// Unix epoch seconds at which `access_token` stops being usable.
    #[serde(default)]
    pub expires_at: Option<u64>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub client_secret: Option<String>,
}

fn yes() -> bool {
    true
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

fn format_of(json: bool) -> BodyFormat {
    if json { BodyFormat::Json } else { BodyFormat::Form }
}

/// Read a plugin's slot, or `None` when it is empty.
fn load(plugin: &str, slot: &str) -> Result<Option<StoredTokens>, String> {
    let account =
        arbor_plugin_types::prelude::credential_account(plugin, slot).map_err(|e| e.to_string())?;
    let raw = crate::auth::credential_store::get(&account, "").map_err(|e| e.to_string())?;
    match raw {
        None => Ok(None),
        Some(s) => serde_json::from_str(&s)
            .map(Some)
            .map_err(|e| format!("arbor.oauth: the '{slot}' slot does not hold tokens ({e})")),
    }
}

/// Write a plugin's slot.
fn save(plugin: &str, slot: &str, tokens: &StoredTokens) -> Result<(), String> {
    let account =
        arbor_plugin_types::prelude::credential_account(plugin, slot).map_err(|e| e.to_string())?;
    let body = serde_json::to_string(tokens).map_err(|e| e.to_string())?;
    crate::auth::credential_store::save(&account, "", &body).map_err(|e| e.to_string())
}

/// Begin an installed-app flow. Returns the URL to open in the browser.
///
/// The wait for the callback runs in the background, so a plugin is never blocked on a person:
/// the outcome arrives as the `on_done` hook, on the plugin's own host.
pub async fn start(app: AppHandle, plugin: String, spec: StartSpec) -> Result<String, String> {
    if spec.client_id.trim().is_empty() {
        return Err("arbor.oauth.start: `client_id` is required".to_string());
    }

    let flow = InstalledAppFlow {
        auth_url: spec.auth_url.clone(),
        token_url: spec.token_url.clone(),
        client_id: spec.client_id.clone(),
        client_secret: spec.client_secret.clone(),
        scope: spec.scope.clone(),
        redirect_port: spec.redirect_port,
        extra_authorize_params: spec.extra_params.clone(),
        token_request_format: format_of(spec.json_token_request),
        provider_label: if spec.label.is_empty() { plugin.clone() } else { spec.label.clone() },
        success_html: None,
        error_html_template: None,
    };

    let (auth_url, pending) =
        flow.start().await.map_err(|e| format!("arbor.oauth.start: {e}"))?;

    if spec.open_browser {
        // A failure here is not the flow's failure: the listener is bound and the URL is
        // valid, so the user can still be handed it. Logged rather than returned.
        if let Err(e) = app.opener().open_url(&auth_url, None::<&str>) {
            tracing::warn!("[{plugin}] arbor.oauth: could not open a browser: {e}");
        }
    }

    tauri::async_runtime::spawn(async move {
        let payload = match pending.await_callback().await {
            Ok(token) => persist(&plugin, &spec, token),
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
        };
        if let Some(hook) = spec.on_done.as_deref() {
            let json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
            crate::ipc::fire_plugin_hook_on_backends(&app, &plugin, hook, &json);
        }
    });

    Ok(auth_url)
}

/// Turn a token response into the stored document, and report what happened.
fn persist(plugin: &str, spec: &StartSpec, token: TokenResponse) -> serde_json::Value {
    let refresh = token.refresh_token.clone().unwrap_or_default();
    if refresh.is_empty() && spec.require_refresh_token {
        return serde_json::json!({
            "ok": false,
            "error": "the provider returned no refresh token — revoke this app's existing \
                      grant and authorize again, or set require_refresh_token = false"
        });
    }
    let tokens = StoredTokens {
        refresh_token: refresh,
        access_token: Some(token.access_token),
        expires_at: token.expires_in.map(|s| now_secs() + (s.max(0) as u64).saturating_sub(30)),
        client_id: Some(spec.client_id.clone()),
        client_secret: spec.client_secret.clone(),
    };
    match save(plugin, &spec.slot, &tokens) {
        Ok(()) => serde_json::json!({ "ok": true, "slot": spec.slot }),
        Err(e) => {
            tracing::error!("[{plugin}] arbor.oauth: saving tokens: {e}");
            serde_json::json!({ "ok": false, "error": e })
        }
    }
}

/// Exchange the stored refresh token for a fresh access token, and put it back.
///
/// Answers `{ refreshed, expires_in }` — `refreshed: false` when the stored token still had
/// enough life left, which is what lets a caller ask before every request without a round trip
/// to the provider each time.
pub async fn refresh(plugin: String, spec: RefreshSpec) -> Result<serde_json::Value, String> {
    let mut tokens = load(&plugin, &spec.slot)?
        .ok_or_else(|| format!("arbor.oauth.refresh: the '{}' slot is empty", spec.slot))?;

    let remaining = tokens.expires_at.map(|at| at.saturating_sub(now_secs())).unwrap_or(0);
    if tokens.access_token.is_some() && remaining > spec.min_remaining_secs {
        return Ok(serde_json::json!({ "refreshed": false, "expires_in": remaining }));
    }
    if tokens.refresh_token.is_empty() {
        return Err(format!(
            "arbor.oauth.refresh: the '{}' slot holds no refresh token — authorize again",
            spec.slot
        ));
    }
    let client_id = tokens.client_id.clone().unwrap_or_default();
    if client_id.is_empty() {
        return Err(format!(
            "arbor.oauth.refresh: the '{}' slot holds no client_id — authorize again",
            spec.slot
        ));
    }

    let fresh = refresh_token(
        &spec.token_url,
        &client_id,
        tokens.client_secret.as_deref(),
        &tokens.refresh_token,
        format_of(spec.json_token_request),
    )
    .await
    .map_err(|e| format!("arbor.oauth.refresh: {e}"))?;

    let expires_in = fresh.expires_in.unwrap_or(0).max(0) as u64;
    tokens.access_token = Some(fresh.access_token);
    tokens.expires_at = Some(now_secs() + expires_in.saturating_sub(30));
    // A provider that rotates refresh tokens returns a new one; one that does not returns
    // nothing, and keeping the old one is what makes both work.
    if let Some(r) = fresh.refresh_token.filter(|r| !r.is_empty()) {
        tokens.refresh_token = r;
    }
    save(&plugin, &spec.slot, &tokens)?;

    Ok(serde_json::json!({ "refreshed": true, "expires_in": expires_in }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_start_spec_is_provider_agnostic_but_insists_on_a_refresh_token() {
        // The default matters: a flow that quietly finishes without a refresh token looks
        // like a success and fails an hour later, when the access token expires and there is
        // nothing to renew it with.
        let spec: StartSpec = serde_json::from_value(serde_json::json!({
            "slot": "oauth",
            "auth_url": "https://example.test/authorize",
            "token_url": "https://example.test/token",
            "client_id": "abc",
            "redirect_port": 7732,
        }))
        .unwrap();
        assert!(spec.require_refresh_token);
        assert!(spec.extra_params.is_empty());
        assert!(!spec.json_token_request);
        assert!(spec.on_done.is_none());
    }

    #[test]
    fn the_stored_document_round_trips() {
        // This shape is a contract with two readers that are not this file: the guest that
        // reads `access_token` out of the slot, and the next refresh.
        let t = StoredTokens {
            refresh_token: "r".into(),
            access_token: Some("a".into()),
            expires_at: Some(42),
            client_id: Some("c".into()),
            client_secret: None,
        };
        let back: StoredTokens = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(back.refresh_token, "r");
        assert_eq!(back.access_token.as_deref(), Some("a"));
        assert_eq!(back.expires_at, Some(42));
        assert_eq!(back.client_id.as_deref(), Some("c"));
    }

    #[test]
    fn a_slot_holding_something_else_says_so() {
        // A plugin that wrote its own string into the slot with `arbor.credentials.set` gets
        // told what is wrong, rather than "no refresh token" three calls later.
        let err = serde_json::from_str::<StoredTokens>("not json").unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
