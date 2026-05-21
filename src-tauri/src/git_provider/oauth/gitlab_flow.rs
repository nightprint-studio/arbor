//! GitLab OAuth 2.0 Authorization Code flow with PKCE.
//!
//! GitLab supports RFC 7636 PKCE since version 12.3. We use the standard
//! Authorization Code flow with a temporary localhost HTTP server to receive
//! the redirect callback — the same approach used for Linear and Jira.
//!
//! Flow:
//!   1. `start_gitlab_oauth(app_handle)` — generates PKCE verifier/challenge,
//!      spawns a one-shot TCP listener on port 7731, returns the authorization URL.
//!   2. Frontend calls `open_url(auth_url)` → user authenticates in their browser.
//!   3. GitLab redirects to `http://127.0.0.1:7731/callback?code=…&state=…`.
//!   4. The background listener exchanges the code for a token via POST to
//!      `https://gitlab.com/oauth/token`.
//!   5. Token (+ refresh token) is stored in the OS keychain.
//!      The Tauri event `arbor://gitlab-oauth-done` (bool) is emitted.
//!
//! # GitLab OAuth application setup
//! Register an application at: gitlab.com → Preferences → Applications
//! (or Admin → Applications for instance-wide). Settings:
//!   - Redirect URI: `http://127.0.0.1:7731/callback`
//!   - Scopes: `api`
//!   - Confidential: No (public client — PKCE is used instead of a secret)

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::Deserialize;

use crate::auth::credential_store;
use crate::error::{AppError, Result};

/// Keychain slot — shared with the PAT flow so the CI client, remote commands,
/// and any other GitLab API callers work regardless of how the user authenticated.
const KR_HOST: &str  = "gitlab.com/arbor";
const KR_USER: &str  = "oauth";
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

// ── PKCE helpers ─────────────────────────────────────────────────────────────

fn code_verifier() -> String {
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

fn urlencoded(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => vec![c as u8],
            _ => format!("%{:02X}", c as u32).into_bytes(),
        })
        .map(|b| b as char)
        .collect()
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Start the GitLab OAuth 2.0 Authorization Code + PKCE flow.
///
/// Spawns a background task that waits for the browser callback, exchanges
/// the code for a token, persists it (plus the refresh token) in the keychain,
/// and emits `arbor://gitlab-oauth-done` (payload: `true` on success, `false` on error).
///
/// Returns the authorization URL that the frontend should open in the default browser.
pub async fn start_gitlab_oauth(app_handle: tauri::AppHandle) -> Result<String> {
    let (client_id, base_host) = resolve_overrides();
    let verifier  = code_verifier();
    let challenge = code_challenge(&verifier);
    let state     = random_state();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| AppError::Other(format!(
            "cannot start OAuth callback server on port {CALLBACK_PORT}: {e}\n\
             Make sure nothing else is using that port."
        )))?;

    let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}/callback");
    let auth_url = format!(
        "https://{base_host}/oauth/authorize\
         ?client_id={client_id}\
         &redirect_uri={redir}\
         &response_type=code\
         &state={state}\
         &scope=api\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        redir = urlencoded(&redirect_uri),
    );

    let app  = app_handle.clone();
    let redir = redirect_uri.clone();
    let ver  = verifier.clone();
    let st   = state.clone();
    let cid  = client_id.clone();
    let bh   = base_host.clone();

    // Payload: null = success, string = error message.
    tokio::spawn(async move {
        let result: Option<String> = match wait_for_callback(listener, &ver, &st, &redir, &cid, &bh).await {
            Ok((access_token, refresh_token)) => {
                match credential_store::save(KR_HOST, KR_USER, &access_token) {
                    Ok(_) => {
                        if let Some(rt) = refresh_token {
                            if let Err(e) = credential_store::save(KR_REFRESH, KR_USER, &rt) {
                                tracing::warn!("gitlab oauth: failed to save refresh token: {e}");
                            }
                        }
                        None // success
                    }
                    Err(e) => {
                        tracing::error!("gitlab oauth: keychain save failed: {e}");
                        Some(format!("Keychain save failed: {e}"))
                    }
                }
            }
            Err(e) => {
                tracing::error!("gitlab oauth callback error: {e}");
                Some(e.to_string())
            }
        };
        let _ = app.emit("arbor://gitlab-oauth-done", result);
    });

    Ok(auth_url)
}

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

async fn wait_for_callback(
    listener: tokio::net::TcpListener,
    verifier: &str,
    expected_state: &str,
    redirect_uri: &str,
    client_id: &str,
    base_host: &str,
) -> Result<(String, Option<String>)> {
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
        send_html(&mut stream, &oauth_error_html("GitLab", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    let code = extract_param(&request, "code")
        .ok_or_else(|| AppError::AuthFailed("no 'code' in OAuth callback".into()))?;
    let recv_state = extract_param(&request, "state")
        .ok_or_else(|| AppError::AuthFailed("no 'state' in OAuth callback".into()))?;

    if recv_state != expected_state {
        let msg = "State mismatch — possible CSRF attack, authorization rejected.".to_string();
        send_html(&mut stream, &oauth_error_html("GitLab", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    // Exchange code for token — serve HTML only after we know the outcome.
    match exchange_code(&code, verifier, redirect_uri, client_id, base_host).await {
        Ok(tokens) => {
            send_html(&mut stream, &oauth_success_html("GitLab")).await;
            Ok(tokens)
        }
        Err(e) => {
            send_html(&mut stream, &oauth_error_html("GitLab", &e.to_string())).await;
            Err(e)
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token:  String,
    refresh_token: Option<String>,
}

async fn exchange_code(
    code: &str,
    verifier: &str,
    redirect_uri: &str,
    client_id: &str,
    base_host: &str,
) -> Result<(String, Option<String>)> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://{base_host}/oauth/token"))
        .header("Accept", "application/json")
        .form(&[
            ("client_id",     client_id),
            ("code",          code),
            ("redirect_uri",  redirect_uri),
            ("grant_type",    "authorization_code"),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("gitlab token exchange request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!("gitlab token exchange {status}: {body}")));
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("gitlab token response parse: {e}")))?;

    Ok((tr.access_token, tr.refresh_token))
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

    let Some(refresh_token) = credential_store::get(KR_REFRESH, KR_USER)? else {
        return Ok(false);
    };

    let (client_id, base_host) = resolve_overrides();
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("https://{base_host}/oauth/token"))
        .header("Accept", "application/json")
        .form(&[
            ("client_id",     client_id.as_str()),
            ("grant_type",    "refresh_token"),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|e| AppError::Other(format!("gitlab token refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        tracing::warn!("gitlab token refresh {status}: {body}");
        return Ok(false);
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("gitlab token refresh parse: {e}")))?;

    credential_store::save(KR_HOST, KR_USER, &tr.access_token)?;

    if let Some(new_rt) = tr.refresh_token {
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
