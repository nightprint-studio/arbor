//! Google OAuth 2.0 installed-app flow with PKCE — backs `GcsAuth::Oauth`.
//!
//! Pattern mirrors `auth/oauth_jira.rs` exactly: PKCE verifier/challenge,
//! random state, tokio TcpListener on a fixed loopback port, HTML success/
//! error page, code → token exchange. The differences:
//!
//!   * Loopback port is `7732` (Jira uses 7730, Linear 7729).
//!   * Google requires the OAuth client_id (and accepts client_secret even
//!     for installed apps). Arbor doesn't ship a default client_id for this
//!     — the user provides theirs via the plugin's connection form.
//!   * Tokens are stored in the cloud-storage keyring (`cloud::secrets`)
//!     rather than `credential_store`, keyed by the plugin-chosen
//!     `secret_ref` (e.g. `"gcs/cfg_abc"`).
//!
//! Earmarked for deletion when the cloud-storage plugin moves to WASM.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use serde::Deserialize;
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{AppError, Result};
use crate::cloud::secrets;
use crate::cloud::auth_gcs::{StoredOAuth, now_secs};

const AUTH_URL:    &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL:   &str = "https://oauth2.googleapis.com/token";
pub const CALLBACK_PORT: u16 = 7732;
const SCOPE:       &str = "https://www.googleapis.com/auth/devstorage.read_write";

const PLUGIN_NAME:    &str = "cloud-storage";
const HOOK_OAUTH_DONE: &str = "cloud-storage:oauth-done";

fn fire_plugin_hook(app: &tauri::AppHandle, payload: serde_json::Value) {
    let state = app.state::<crate::AppState>();
    let json  = match serde_json::to_string(&payload) {
        Ok(s)  => s,
        Err(e) => { tracing::warn!("cloud oauth hook encode: {e}"); return; }
    };
    if let Ok(host) = state.lock_plugin_host() {
        let _ = host.fire_hook_on(PLUGIN_NAME, HOOK_OAUTH_DONE, &json);
    };
}

// ── PKCE helpers (same shape as oauth_jira.rs) ──────────────────────────────

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

fn extract_param(request: &str, key: &str) -> Option<String> {
    let line = request.lines().next()?;
    let query = line.split('?').nth(1)?.split_whitespace().next()?;
    for pair in query.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some(key) { return parts.next().map(percent_decode); }
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
                out.push(b as char); continue;
            }
            out.push('%'); out.push(h1); out.push(h2);
        } else if c == '+' { out.push(' '); }
        else { out.push(c); }
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

// ── HTML callback pages ────────────────────────────────────────────────────

async fn send_html(stream: &mut tokio::net::TcpStream, body: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(), body
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

fn success_html() -> String {
    SUCCESS_HTML.to_string()
}
fn error_html(message: &str) -> String {
    ERROR_HTML.replace("__MSG__", message)
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><title>Arbor — Connected</title><style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;background:#1e1f22;color:#dfe1e5;min-height:100vh;display:flex;align-items:center;justify-content:center}
.card{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;padding:44px 52px;max-width:420px;text-align:center}
.brand{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;color:#6c707a;margin-bottom:32px}
.icon{width:56px;height:56px;border-radius:50%;background:#1a3326;border:2px solid #6aab73;display:flex;align-items:center;justify-content:center;margin:0 auto 20px;font-size:24px}
h1{font-size:18px;color:#6aab73;margin-bottom:10px}p{font-size:13px;color:#888d94}
</style></head><body><div class="card"><div class="brand">Arbor</div><div class="icon">✓</div>
<h1>Connected successfully</h1><p>Google Cloud Storage has been authorized.<br>You can close this tab and return to Arbor.</p></div></body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="UTF-8"><title>Arbor — Authorization failed</title><style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',system-ui,sans-serif;background:#1e1f22;color:#dfe1e5;min-height:100vh;display:flex;align-items:center;justify-content:center}
.card{background:#2b2d30;border:1px solid #3c3f41;border-radius:12px;padding:44px 52px;max-width:460px;text-align:center}
.brand{font-size:11px;font-weight:700;letter-spacing:.18em;text-transform:uppercase;color:#6c707a;margin-bottom:32px}
.icon{width:56px;height:56px;border-radius:50%;background:#2d1f1f;border:2px solid #f87171;display:flex;align-items:center;justify-content:center;margin:0 auto 20px;font-size:24px}
h1{font-size:18px;color:#f87171;margin-bottom:10px}p{font-size:13px;color:#888d94}
.detail{margin-top:20px;padding:12px 14px;background:#1e1f22;border:1px solid #3c3f41;border-radius:6px;font-size:11px;color:#888d94;font-family:'JetBrains Mono',monospace;text-align:left;word-break:break-all}
</style></head><body><div class="card"><div class="brand">Arbor</div><div class="icon">✗</div>
<h1>Authorization failed</h1><p>Could not connect Google Cloud Storage.</p><div class="detail">__MSG__</div></div></body></html>"#;

// ── Public API ─────────────────────────────────────────────────────────────

/// Start the OAuth flow. Returns the authorization URL the frontend opens
/// in the browser. A background task awaits the loopback callback, exchanges
/// the code for tokens, persists them under `secret_ref`, and emits
/// `arbor://cloud-oauth-done` with `{ok: bool, error?: string}`.
pub async fn start(
    app_handle:    tauri::AppHandle,
    secret_ref:    String,
    client_id:     String,
    client_secret: Option<String>,
) -> Result<String> {
    if client_id.trim().is_empty() {
        return Err(AppError::AuthFailed(
            "Google OAuth requires a client_id. Register a Desktop OAuth client at \
             https://console.cloud.google.com/apis/credentials and paste its client_id \
             (and client_secret) into the connection form.".into()
        ));
    }

    let verifier  = code_verifier();
    let challenge = code_challenge(&verifier);
    let state     = random_state();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| AppError::Other(format!(
            "cannot start Google OAuth callback server on port {CALLBACK_PORT}: {e}\n\
             Make sure nothing else is using that port."
        )))?;

    let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}/callback");
    let auth_url = format!(
        "{AUTH_URL}\
         ?client_id={cid}\
         &response_type=code\
         &redirect_uri={redir}\
         &scope={scope}\
         &state={state}\
         &access_type=offline\
         &prompt=consent\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        cid    = urlencoded(&client_id),
        redir  = urlencoded(&redirect_uri),
        scope  = urlencoded(SCOPE),
    );

    let app      = app_handle.clone();
    let ver      = verifier.clone();
    let st       = state.clone();
    let cid      = client_id.clone();
    let csec     = client_secret.clone();
    let redir    = redirect_uri.clone();
    let ref_key  = secret_ref.clone();

    tokio::spawn(async move {
        let outcome = wait_for_callback(listener, &ver, &st, &cid, csec.as_deref(), &redir).await;
        let payload = match outcome {
            Ok(token) => {
                let stored = StoredOAuth {
                    refresh_token: token.refresh_token.clone().unwrap_or_default(),
                    access_token:  Some(token.access_token),
                    expires_at:    Some(now_secs() + token.expires_in.saturating_sub(30) as u64),
                    client_id:     Some(cid),
                    client_secret: csec,
                };
                if stored.refresh_token.is_empty() {
                    serde_json::json!({
                        "ok": false,
                        "error": "Google did not return a refresh token. \
                                  Reset the app's grant at https://myaccount.google.com/permissions \
                                  and try again."
                    })
                } else {
                    let body = match serde_json::to_string(&stored) {
                        Ok(s) => s,
                        Err(e) => return emit_err(&app, format!("encode tokens: {e}")),
                    };
                    if let Err(e) = secrets::set(&ref_key, &body) {
                        return emit_err(&app, format!("save tokens to keyring: {e}"));
                    }
                    serde_json::json!({ "ok": true, "secret_ref": ref_key })
                }
            }
            Err(e) => serde_json::json!({ "ok": false, "error": e.to_string() }),
        };
        fire_plugin_hook(&app, payload);
    });

    Ok(auth_url)
}

/// Exchange the stored refresh token for a fresh access token.
/// Called by `auth_gcs::resolve` when the cached access token is missing or
/// near expiry. Pure HTTP — no UI / browser involvement.
pub async fn refresh_with(stored: &StoredOAuth) -> Result<RefreshedToken> {
    let client_id = stored.client_id.as_deref().unwrap_or("");
    if client_id.is_empty() {
        return Err(AppError::AuthFailed(
            "OAuth client_id missing from stored token blob — re-run the authorization flow".into()
        ));
    }

    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type",    "refresh_token"),
        ("refresh_token", stored.refresh_token.as_str()),
        ("client_id",     client_id),
    ];
    if let Some(secret) = stored.client_secret.as_deref() {
        if !secret.is_empty() { form.push(("client_secret", secret)); }
    }

    let resp = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::AuthFailed(format!("Google refresh request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!("Google refresh HTTP {status}: {body}")));
    }
    #[derive(Deserialize)]
    struct R { access_token: String, #[serde(default)] expires_in: i64 }
    let r: R = resp.json().await
        .map_err(|e| AppError::AuthFailed(format!("Google refresh decode: {e}")))?;
    Ok(RefreshedToken {
        access_token:    r.access_token,
        expires_in_secs: r.expires_in.max(0) as u64,
    })
}

pub struct RefreshedToken {
    pub access_token:    String,
    pub expires_in_secs: u64,
}

// ── internals ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token:  String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in:    i64,
}

async fn wait_for_callback(
    listener:      tokio::net::TcpListener,
    verifier:      &str,
    expected_state: &str,
    client_id:     &str,
    client_secret: Option<&str>,
    redirect_uri:  &str,
) -> Result<TokenResponse> {
    let (mut stream, _) = listener.accept().await
        .map_err(|e| AppError::Other(format!("accept error: {e}")))?;

    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await
        .map_err(|e| AppError::Other(format!("read error: {e}")))?;
    let request = String::from_utf8_lossy(&buf[..n]).to_string();

    if let Some(err) = extract_param(&request, "error") {
        let desc = extract_param(&request, "error_description").unwrap_or_default();
        let msg = if desc.is_empty() { err.clone() } else { format!("{err}: {desc}") };
        send_html(&mut stream, &error_html(&msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    let code = extract_param(&request, "code")
        .ok_or_else(|| AppError::AuthFailed("no 'code' in OAuth callback".into()))?;
    let recv_state = extract_param(&request, "state")
        .ok_or_else(|| AppError::AuthFailed("no 'state' in OAuth callback".into()))?;
    if recv_state != expected_state {
        let msg = "State mismatch — possible CSRF, authorization rejected.".to_string();
        send_html(&mut stream, &error_html(&msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    match exchange_code(client_id, client_secret, &code, verifier, redirect_uri).await {
        Ok(token) => {
            send_html(&mut stream, &success_html()).await;
            Ok(token)
        }
        Err(e) => {
            send_html(&mut stream, &error_html(&e.to_string())).await;
            Err(e)
        }
    }
}

async fn exchange_code(
    client_id:     &str,
    client_secret: Option<&str>,
    code:          &str,
    verifier:      &str,
    redirect_uri:  &str,
) -> Result<TokenResponse> {
    let mut form: Vec<(&str, &str)> = vec![
        ("grant_type",    "authorization_code"),
        ("client_id",     client_id),
        ("code",          code),
        ("redirect_uri",  redirect_uri),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = client_secret {
        if !secret.is_empty() { form.push(("client_secret", secret)); }
    }

    let resp = reqwest::Client::new()
        .post(TOKEN_URL)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Google token exchange request failed: {e}")))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!("Google token exchange {status}: {body}")));
    }
    let tr: TokenResponse = resp.json().await
        .map_err(|e| AppError::Other(format!("Google token response parse: {e}")))?;
    Ok(tr)
}

fn emit_err(app: &tauri::AppHandle, msg: String) {
    tracing::error!("cloud oauth: {msg}");
    fire_plugin_hook(app, serde_json::json!({ "ok": false, "error": msg }));
}
