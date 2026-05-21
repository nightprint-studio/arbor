//! Linear OAuth 2.0 Authorization Code flow with PKCE.
//!
//! Linear does not support RFC 8628 Device Flow, so we use the standard
//! Authorization Code flow with a temporary localhost HTTP server to receive
//! the redirect callback.
//!
//! Flow:
//!   1. `start_linear_oauth(client_id)` — generates PKCE verifier/challenge,
//!      spawns a one-shot TCP listener, returns the authorization URL.
//!   2. User opens the URL in the browser and grants access.
//!   3. Linear redirects to `http://127.0.0.1:{port}/callback?code=…&state=…`.
//!   4. The background listener exchanges the code for a token via POST to
//!      `https://api.linear.app/oauth/token`.
//!   5. Token is stored at the same keychain slot used by the PAT flow; the
//!      Tauri event `arbor://linear-oauth-done` (bool) is emitted.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::Deserialize;

use crate::auth::credential_store;
use crate::error::{AppError, Result};

/// Keychain slot — shared with the PAT flow so the GraphQL client works
/// regardless of which authentication method was used.
const KR_HOST: &str = "linear.app";
const KR_USER: &str = "api-key";

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

// ── PKCE helpers ─────────────────────────────────────────────────────────────

fn code_verifier() -> String {
    // Use two UUIDs (v4 = 16 random bytes each) for 32 bytes of entropy.
    let a = uuid::Uuid::new_v4();
    let b = uuid::Uuid::new_v4();
    let bytes: Vec<u8> = a.as_bytes().iter().chain(b.as_bytes().iter()).copied().collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn code_challenge(verifier: &str) -> String {
    let mut h = Sha256::new();
    h.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(h.finalize())
}

fn random_state() -> String {
    URL_SAFE_NO_PAD.encode(uuid::Uuid::new_v4().as_bytes())
}

// ── HTTP callback parser ──────────────────────────────────────────────────────

/// Extract a query-string parameter from the raw HTTP request text.
/// The first line looks like `GET /callback?code=X&state=Y HTTP/1.1`.
fn extract_param(request: &str, key: &str) -> Option<String> {
    let line = request.lines().next()?;
    let query = line.split('?').nth(1)?.split_whitespace().next()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) {
            return parts.next().map(percent_decode);
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{h1}{h2}"), 16) {
                out.push(b as char);
                continue;
            }
            out.push('%'); out.push(h1); out.push(h2);
        } else if c == '+' {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

// ── HTML callback pages ───────────────────────────────────────────────────────

async fn send_html(stream: &mut tokio::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn oauth_success_html(provider: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Arbor — Connected</title>
  <style>
    *{{margin:0;padding:0;box-sizing:border-box}}
    body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
      background:#1e1f22;color:#dfe1e5;min-height:100vh;
      display:flex;align-items:center;justify-content:center}}
    .card{{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;
      padding:44px 52px;max-width:420px;width:90%;text-align:center}}
    .brand{{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;
      color:#6c707a;margin-bottom:32px}}
    .icon{{width:56px;height:56px;border-radius:50%;background:#1a3326;
      border:2px solid #6aab73;display:flex;align-items:center;justify-content:center;
      margin:0 auto 20px;font-size:24px}}
    h1{{font-size:18px;font-weight:600;color:#6aab73;margin-bottom:10px}}
    p{{font-size:13px;color:#888d94;line-height:1.6}}
    .provider{{color:#dfe1e5;font-weight:500}}
  </style>
</head>
<body>
  <div class="card">
    <div class="brand">Arbor</div>
    <div class="icon">✓</div>
    <h1>Connected successfully</h1>
    <p><span class="provider">{provider}</span> has been authorized.<br>You can close this tab and return to Arbor.</p>
  </div>
</body>
</html>"#)
}

fn oauth_error_html(provider: &str, message: &str) -> String {
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Arbor — Authorization failed</title>
  <style>
    *{{margin:0;padding:0;box-sizing:border-box}}
    body{{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;
      background:#1e1f22;color:#dfe1e5;min-height:100vh;
      display:flex;align-items:center;justify-content:center}}
    .card{{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;
      padding:44px 52px;max-width:460px;width:90%;text-align:center}}
    .brand{{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;
      color:#6c707a;margin-bottom:32px}}
    .icon{{width:56px;height:56px;border-radius:50%;background:#2d1f1f;
      border:2px solid #f87171;display:flex;align-items:center;justify-content:center;
      margin:0 auto 20px;font-size:24px}}
    h1{{font-size:18px;font-weight:600;color:#f87171;margin-bottom:10px}}
    p{{font-size:13px;color:#888d94;line-height:1.6}}
    .provider{{color:#dfe1e5;font-weight:500}}
    .detail{{margin-top:20px;padding:12px 14px;background:#1e1f22;
      border:1px solid #3c3f41;border-radius:6px;font-size:11px;
      color:#888d94;font-family:'JetBrains Mono','Fira Code',monospace;
      text-align:left;word-break:break-all;line-height:1.5}}
  </style>
</head>
<body>
  <div class="card">
    <div class="brand">Arbor</div>
    <div class="icon">✗</div>
    <h1>Authorization failed</h1>
    <p>Could not connect <span class="provider">{provider}</span>. Please return to Arbor and try again.</p>
    <div class="detail">{message}</div>
  </div>
</body>
</html>"#)
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
    let verifier  = code_verifier();
    let challenge = code_challenge(&verifier);
    let state     = random_state();

    // Bind on the fixed callback port. If something else is using it, we fail
    // with a clear error rather than silently choosing a different port that
    // would not match the redirect URI registered on linear.app.
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| AppError::Other(format!(
            "cannot start OAuth callback server on port {CALLBACK_PORT}: {e}\n\
             Make sure nothing else is using that port."
        )))?;

    let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}/callback");
    let scope = "read%20write%20issues%3Acreate%20comments%3Acreate";
    let auth_url = format!(
        "https://linear.app/oauth/authorize\
         ?response_type=code\
         &client_id={client_id}\
         &redirect_uri={redir}\
         &scope={scope}\
         &state={state}\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        redir = urlencoded(&redirect_uri),
    );

    let app   = app_handle.clone();
    let redir = redirect_uri.clone();
    let ver   = verifier.clone();
    let st    = state.clone();
    let cid   = client_id.clone();

    tokio::spawn(async move {
        match wait_for_callback(listener, &ver, &st, &cid, &redir).await {
            Ok(token) => {
                match credential_store::save(KR_HOST, KR_USER, &token) {
                    Ok(_)  => { let _ = app.emit("arbor://linear-oauth-done", true); }
                    Err(e) => {
                        tracing::error!("linear oauth: keychain save failed: {e}");
                        let _ = app.emit("arbor://linear-oauth-done", false);
                    }
                }
            }
            Err(e) => {
                tracing::error!("linear oauth callback error: {e}");
                let _ = app.emit("arbor://linear-oauth-done", false);
            }
        }
    });

    Ok(auth_url)
}

async fn wait_for_callback(
    listener: tokio::net::TcpListener,
    verifier: &str,
    expected_state: &str,
    client_id: &str,
    redirect_uri: &str,
) -> Result<String> {
    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| AppError::Other(format!("accept error: {e}")))?;

    let mut buf = vec![0u8; 8192];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| AppError::Other(format!("read error: {e}")))?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    // Provider-side error (e.g. user clicked "Deny").
    if let Some(err) = extract_param(&request, "error") {
        let desc = extract_param(&request, "error_description").unwrap_or_default();
        let msg = if desc.is_empty() { err.clone() } else { format!("{err}: {desc}") };
        send_html(&mut stream, &oauth_error_html("Linear", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    let code = extract_param(&request, "code")
        .ok_or_else(|| AppError::AuthFailed("no 'code' in OAuth callback".into()))?;
    let recv_state = extract_param(&request, "state")
        .ok_or_else(|| AppError::AuthFailed("no 'state' in OAuth callback".into()))?;

    if recv_state != expected_state {
        let msg = "State mismatch — possible CSRF attack, authorization rejected.".to_string();
        send_html(&mut stream, &oauth_error_html("Linear", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    // Exchange code for token — serve HTML only after we know the outcome.
    match exchange_code(client_id, &code, verifier, redirect_uri).await {
        Ok(token) => {
            send_html(&mut stream, &oauth_success_html("Linear")).await;
            Ok(token)
        }
        Err(e) => {
            send_html(&mut stream, &oauth_error_html("Linear", &e.to_string())).await;
            Err(e)
        }
    }
}

const KR_REFRESH: &str = "linear.app-refresh";

#[derive(Deserialize)]
struct TokenResponse {
    access_token:  String,
    refresh_token: Option<String>,
}

async fn exchange_code(
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.linear.app/oauth/token")
        .form(&[
            ("client_id",     client_id),
            ("code",          code),
            ("redirect_uri",  redirect_uri),
            ("grant_type",    "authorization_code"),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("token exchange request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!("token exchange {status}: {body}")));
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("token response parse: {e}")))?;

    if let Some(rt) = tr.refresh_token {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &rt) {
            tracing::warn!("linear oauth: failed to save refresh token: {e}");
        }
    }

    Ok(tr.access_token)
}

/// Attempt to obtain a fresh access token using the stored refresh token.
///
/// Returns `true` when a new access token was obtained and persisted,
/// `false` when no refresh token is stored (PAT flow, or not yet authed via OAuth).
pub async fn try_refresh() -> Result<bool> {
    let Some(refresh_token) = credential_store::get(KR_REFRESH, KR_USER)? else {
        return Ok(false);
    };

    let client_id = resolve_client_id();
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.linear.app/oauth/token")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("grant_type",    "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("linear token refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        tracing::warn!("linear token refresh {status}: {body}");
        return Ok(false);
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("linear token refresh parse: {e}")))?;

    credential_store::save(KR_HOST, KR_USER, &tr.access_token)?;

    if let Some(new_rt) = tr.refresh_token {
        if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &new_rt) {
            tracing::warn!("linear oauth: failed to update refresh token: {e}");
        }
    }

    tracing::debug!("linear access token refreshed successfully");
    Ok(true)
}

/// Percent-encode a string for use in URL query parameters.
fn urlencoded(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                vec![c as u8]
            }
            _ => format!("%{:02X}", c as u32).into_bytes(),
        })
        .map(|b| b as char)
        .collect()
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
