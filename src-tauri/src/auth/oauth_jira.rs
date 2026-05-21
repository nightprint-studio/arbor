//! Jira OAuth 2.0 (3LO) Authorization Code + PKCE flow.
//! Also provides Basic Auth (email + Atlassian API token) helpers.
//!
//! OAuth flow (requires an Atlassian OAuth 2.0 app registered at developer.atlassian.com):
//!   1. `start_jira_oauth(client_id)` — PKCE verifier/challenge, spawns callback listener,
//!      returns the authorization URL.
//!   2. User approves in browser.
//!   3. Atlassian redirects to `http://127.0.0.1:7730/callback?code=…`.
//!   4. Exchange code for token via the Atlassian token endpoint.
//!   5. Fetch accessible cloud resources to obtain the cloud ID.
//!   6. Persist access token, refresh token, cloud ID in keychain.
//!   7. Emit `arbor://jira-oauth-done` (bool).
//!
//! Basic Auth: user provides Atlassian email + API token + Jira subdomain.
//! API token can be generated at https://id.atlassian.com/manage-profile/security/api-tokens.

use base64::{Engine, engine::general_purpose::{URL_SAFE_NO_PAD, STANDARD}};
use sha2::{Digest, Sha256};
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::Deserialize;

use crate::auth::credential_store;
use crate::error::{AppError, Result};

// ── Keychain keys ─────────────────────────────────────────────────────────────
//
// The credential_store uses only the first argument (host) as the actual keyring
// key — the second (username) is ignored. We therefore encode the semantic name
// directly into the key string so each piece of data gets its own keychain slot.

const KR_TOKEN:   &str = "arbor-jira-token";
const KR_REFRESH: &str = "arbor-jira-refresh";
const KR_CLOUD:   &str = "arbor-jira-cloud-id";
const KR_EMAIL:   &str = "arbor-jira-email";
const KR_APIKEY:  &str = "arbor-jira-api-key";
const KR_DOMAIN:  &str = "arbor-jira-domain";

/// Bundled Atlassian OAuth 2.0 app Client ID.  Users can override this in
/// `~/.config/arbor/config.toml` under `[oauth.jira] client_id = "..."` to
/// point Arbor at their own OAuth 2.0 (3LO) app.
/// Register the app at: developer.atlassian.com → OAuth 2.0 (3LO)
/// Redirect URI: http://127.0.0.1:7730/callback
/// Scopes: read:jira-work write:jira-work offline_access read:me
pub const DEFAULT_CLIENT_ID: &str = "jV9R7QAut4zXzxT62ANeqecZTObAmXmm";

/// Loopback port for the OAuth callback — distinct from Linear (7729).
const CALLBACK_PORT: u16 = 7730;

/// Resolve the active Jira `client_id` — config override or bundled default.
fn resolve_client_id() -> String {
    crate::config::app_config::OAuthOverrides::load_from_disk()
        .jira
        .client_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string())
}

// ── Atlassian OAuth 2.0 endpoints ────────────────────────────────────────────

const AUTH_URL:      &str = "https://auth.atlassian.com/authorize";
const TOKEN_URL:     &str = "https://auth.atlassian.com/oauth/token";
const RESOURCES_URL: &str = "https://api.atlassian.com/oauth/token/accessible-resources";

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

// ── HTTP callback helpers ────────────────────────────────────────────────────

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

// ── Public configuration struct ───────────────────────────────────────────────

/// Resolved Jira connection config used by the integration layer.
#[derive(Debug, Clone)]
pub struct JiraConfig {
    /// REST API base, e.g.:
    ///   OAuth: "https://api.atlassian.com/ex/jira/{cloudId}/rest/api/2"
    ///   Basic: "https://mycompany.atlassian.net/rest/api/2"
    pub base_url:    String,
    /// "Bearer {token}" for OAuth, "Basic {b64}" for Basic.
    pub auth_header: String,
    /// "oauth" | "basic"
    pub auth_method: String,
    /// Human-readable host, e.g. "mycompany.atlassian.net"
    pub domain:      Option<String>,
    /// Agile REST base (used for sprints), derived from base_url.
    pub agile_url:   String,
}

/// Pick REST API version based on domain: Cloud (.atlassian.net) → v3, Server/DC → v2.
fn api_version(domain: &str) -> &'static str {
    if domain.ends_with(".atlassian.net") { "3" } else { "2" }
}

/// Derive the Jira Agile REST base from the main REST base URL.
fn agile_url_from(base: &str) -> String {
    if let Some(pos) = base.rfind("/rest/api/") {
        format!("{}/rest/agile/1.0", &base[..pos])
    } else {
        base.to_string()
    }
}

/// Returns the active Jira config, or `None` when no credentials are stored.
pub fn get_config() -> Result<Option<JiraConfig>> {
    // OAuth path — cloud-id present means OAuth was used.
    if let (Some(cloud_id), Some(token)) = (
        credential_store::get(KR_CLOUD, "v")?,
        credential_store::get(KR_TOKEN, "v")?,
    ) {
        let base = format!("https://api.atlassian.com/ex/jira/{cloud_id}/rest/api/2");
        let domain = credential_store::get(KR_DOMAIN, "v")?
            .map(|d| format!("{d}.atlassian.net"));
        return Ok(Some(JiraConfig {
            agile_url:   agile_url_from(&base),
            base_url:    base,
            auth_header: format!("Bearer {token}"),
            auth_method: "oauth".into(),
            domain,
        }));
    }
    // Basic/PAT path — domain + api-key required; email only for Jira Cloud.
    if let (Some(api_token), Some(domain)) = (
        credential_store::get(KR_APIKEY, "v")?,
        credential_store::get(KR_DOMAIN, "v")?,
    ) {
        let is_cloud = domain.ends_with(".atlassian.net");
        let ver = api_version(&domain);
        let base = format!("https://{domain}/rest/api/{ver}");
        let auth_header = if is_cloud {
            // Jira Cloud: Basic Auth with email:api_token
            let email = credential_store::get(KR_EMAIL, "v")?.unwrap_or_default();
            let encoded = STANDARD.encode(format!("{email}:{api_token}"));
            format!("Basic {encoded}")
        } else {
            // Jira Data Center / Server: Personal Access Token as Bearer
            format!("Bearer {api_token}")
        };
        return Ok(Some(JiraConfig {
            agile_url:   agile_url_from(&base),
            base_url:    base,
            auth_method: if is_cloud { "basic".into() } else { "pat".into() },
            auth_header,
            domain:      Some(domain),
        }));
    }
    Ok(None)
}

/// Returns `true` when any Jira credentials are present in the keychain.
pub fn get_status() -> Result<bool> {
    Ok(get_config()?.is_some())
}

/// Remove all Jira credentials from the keychain.
pub fn disconnect() -> Result<()> {
    let _ = credential_store::delete(KR_TOKEN,   "v");
    let _ = credential_store::delete(KR_REFRESH, "v");
    let _ = credential_store::delete(KR_CLOUD,   "v");
    let _ = credential_store::delete(KR_EMAIL,   "v");
    let _ = credential_store::delete(KR_APIKEY,  "v");
    let _ = credential_store::delete(KR_DOMAIN,  "v");
    Ok(())
}

// ── Basic Auth ────────────────────────────────────────────────────────────────

/// Save Basic Auth credentials (email + Atlassian API token + Jira subdomain).
/// The `domain` should be the Jira Cloud subdomain (e.g. "mycompany" for
/// mycompany.atlassian.net) or a full hostname for Jira Server.
pub fn save_basic_auth(email: &str, api_token: &str, domain: &str) -> Result<()> {
    let is_cloud = domain.trim().ends_with(".atlassian.net");
    if is_cloud && !email.is_empty() {
        credential_store::save(KR_EMAIL, "v", email)?;
    }
    credential_store::save(KR_APIKEY, "v", api_token)?;
    let clean = domain.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    credential_store::save(KR_DOMAIN, "v", clean)?;
    // Clear any leftover OAuth credentials.
    let _ = credential_store::delete(KR_TOKEN,   "v");
    let _ = credential_store::delete(KR_REFRESH, "v");
    let _ = credential_store::delete(KR_CLOUD,   "v");
    Ok(())
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

// ── OAuth ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token:  String,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct CloudResource {
    id:   String,
    name: String,
}

/// Start Jira OAuth 2.0 (3LO) + PKCE flow.
/// Returns the authorization URL. The background task emits
/// `arbor://jira-oauth-done` (bool) when finished.
pub async fn start_jira_oauth(app_handle: tauri::AppHandle) -> Result<String> {
    let client_id = resolve_client_id();

    let verifier  = code_verifier();
    let challenge = code_challenge(&verifier);
    let state     = random_state();

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT))
        .await
        .map_err(|e| AppError::Other(format!(
            "cannot start Jira OAuth callback server on port {CALLBACK_PORT}: {e}\n\
             Make sure nothing else is using that port."
        )))?;

    let redirect_uri = format!("http://127.0.0.1:{CALLBACK_PORT}/callback");
    let scope = "read:jira-work%20write:jira-work%20offline_access";
    let auth_url = format!(
        "{AUTH_URL}\
         ?audience=api.atlassian.com\
         &client_id={client_id}\
         &scope={scope}\
         &redirect_uri={redir}\
         &state={state}\
         &response_type=code\
         &prompt=consent\
         &code_challenge={challenge}\
         &code_challenge_method=S256",
        redir = urlencoded(&redirect_uri),
    );

    let app  = app_handle.clone();
    let redir = redirect_uri.clone();
    let ver  = verifier.clone();
    let st   = state.clone();
    let cid  = client_id.clone();

    tokio::spawn(async move {
        match wait_for_callback(listener, &ver, &st, &cid, &redir).await {
            Ok((access_token, refresh_token, cloud_id, domain)) => {
                let save_result = (|| -> Result<()> {
                    credential_store::save(KR_TOKEN, "v", &access_token)?;
                    credential_store::save(KR_CLOUD, "v", &cloud_id)?;
                    if let Some(rt) = &refresh_token {
                        let _ = credential_store::save(KR_REFRESH, "v", rt);
                    }
                    if let Some(d) = &domain {
                        let sub = d.trim_end_matches(".atlassian.net");
                        let _ = credential_store::save(KR_DOMAIN, "v", sub);
                    }
                    Ok(())
                })();
                match save_result {
                    Ok(_)  => { let _ = app.emit("arbor://jira-oauth-done", true); }
                    Err(e) => {
                        tracing::error!("jira oauth: keychain save failed: {e}");
                        let _ = app.emit("arbor://jira-oauth-done", false);
                    }
                }
            }
            Err(e) => {
                tracing::error!("jira oauth callback error: {e}");
                let _ = app.emit("arbor://jira-oauth-done", false);
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
) -> Result<(String, Option<String>, String, Option<String>)> {
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
        send_html(&mut stream, &oauth_error_html("Jira", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    let code = extract_param(&request, "code")
        .ok_or_else(|| AppError::AuthFailed("no 'code' in Jira OAuth callback".into()))?;
    let recv_state = extract_param(&request, "state")
        .ok_or_else(|| AppError::AuthFailed("no 'state' in Jira OAuth callback".into()))?;

    if recv_state != expected_state {
        let msg = "State mismatch — possible CSRF attack, authorization rejected.".to_string();
        send_html(&mut stream, &oauth_error_html("Jira", &msg)).await;
        return Err(AppError::AuthFailed(msg));
    }

    // Exchange code for token — serve HTML only after we know the outcome.
    let result = async {
        let (access_token, refresh_token) =
            exchange_code(client_id, &code, verifier, redirect_uri).await?;
        let (cloud_id, site_name) =
            fetch_cloud_resource(&access_token).await?;
        Ok::<_, AppError>((access_token, refresh_token, cloud_id, Some(site_name)))
    }.await;

    match result {
        Ok(tokens) => {
            send_html(&mut stream, &oauth_success_html("Jira")).await;
            Ok(tokens)
        }
        Err(e) => {
            send_html(&mut stream, &oauth_error_html("Jira", &e.to_string())).await;
            Err(e)
        }
    }
}

async fn exchange_code(
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<(String, Option<String>)> {
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type":    "authorization_code",
            "client_id":     client_id,
            "code":          code,
            "redirect_uri":  redirect_uri,
            "code_verifier": verifier,
        }))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Jira token exchange request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body   = resp.text().await.unwrap_or_default();
        return Err(AppError::AuthFailed(format!("Jira token exchange {status}: {body}")));
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Jira token response parse: {e}")))?;
    Ok((tr.access_token, tr.refresh_token))
}

async fn fetch_cloud_resource(token: &str) -> Result<(String, String)> {
    let client = reqwest::Client::new();
    let resp = client
        .get(RESOURCES_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Jira cloud resources request failed: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::AuthFailed(
            "Failed to fetch Jira cloud resources — no accessible sites?".into(),
        ));
    }

    let resources: Vec<CloudResource> = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Jira cloud resources parse: {e}")))?;

    resources.into_iter().next()
        .map(|r| (r.id, r.name))
        .ok_or_else(|| AppError::AuthFailed(
            "No Jira Cloud sites accessible with this account. \
             Make sure you granted access to at least one site.".into(),
        ))
}

/// Attempt to refresh the Jira OAuth access token using the stored refresh token.
/// Returns `true` when a new token was obtained and stored.
pub async fn try_refresh() -> Result<bool> {
    let client_id = resolve_client_id();
    let Some(refresh_token) = credential_store::get(KR_REFRESH, "v")? else {
        return Ok(false);
    };

    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_URL)
        .json(&serde_json::json!({
            "grant_type":    "refresh_token",
            "client_id":     client_id,
            "refresh_token": refresh_token,
        }))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("Jira token refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        tracing::warn!("jira token refresh failed: {}", resp.status());
        return Ok(false);
    }

    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| AppError::Other(format!("Jira token refresh parse: {e}")))?;

    credential_store::save(KR_TOKEN, "v", &tr.access_token)?;
    if let Some(new_rt) = tr.refresh_token {
        let _ = credential_store::save(KR_REFRESH, "v", &new_rt);
    }

    tracing::debug!("jira access token refreshed successfully");
    Ok(true)
}
