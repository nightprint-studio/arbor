//! Jira OAuth 2.0 (3LO) Authorization Code + PKCE flow.
//! Also provides Basic Auth (email + Atlassian API token) helpers.
//!
//! OAuth flow (requires an Atlassian OAuth 2.0 app registered at
//! developer.atlassian.com):
//!   1. `start_jira_oauth()` configures [`arbor_auth::oauth2::InstalledAppFlow`]
//!      with the Atlassian endpoints + a JSON token body, binds the loopback
//!      listener and returns the authorization URL.
//!   2. User approves in browser.
//!   3. Atlassian redirects to `http://127.0.0.1:7730/callback?code=…`.
//!   4. `arbor-auth` exchanges code → tokens.
//!   5. Jira-specific: fetch accessible cloud resources to obtain the cloud ID.
//!   6. Persist access token, refresh token, cloud ID in keychain.
//!   7. Emit `arbor://jira-oauth-done` (bool).
//!
//! Basic Auth: user provides Atlassian email + API token + Jira subdomain.
//! API token can be generated at https://id.atlassian.com/manage-profile/security/api-tokens.

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use tauri::Emitter;

use arbor_auth::oauth2::{InstalledAppFlow, refresh_token};
use arbor_auth::BodyFormat;

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

/// Loopback port for the OAuth callback — distinct from Linear (7729) and
/// Google Cloud Storage (7732).
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

const SCOPE: &str = "read:jira-work write:jira-work offline_access";

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

// ── OAuth ─────────────────────────────────────────────────────────────────────

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

    let flow = InstalledAppFlow {
        auth_url:               AUTH_URL.into(),
        token_url:              TOKEN_URL.into(),
        client_id,
        // Atlassian 3LO public clients don't send a client_secret.
        client_secret:          None,
        scope:                  SCOPE.into(),
        redirect_port:          CALLBACK_PORT,
        extra_authorize_params: vec![
            ("audience".into(), "api.atlassian.com".into()),
            ("prompt".into(),   "consent".into()),
        ],
        // Atlassian's token endpoint expects a JSON body.
        token_request_format:   BodyFormat::Json,
        provider_label:         "Jira".into(),
        success_html:           None,
        error_html_template:    None,
    };

    let (auth_url, pending) = flow.start().await
        .map_err(|e| AppError::Other(format!("Jira OAuth start: {e}")))?;

    let app = app_handle.clone();
    tokio::spawn(async move {
        let outcome = pending.await_callback().await;
        let ok = match outcome {
            Ok(token) => {
                match persist_oauth_result(&token.access_token, token.refresh_token.as_deref()).await {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::error!("jira oauth: persist failed: {e}");
                        false
                    }
                }
            }
            Err(e) => {
                tracing::error!("jira oauth callback error: {e}");
                false
            }
        };
        let _ = app.emit("arbor://jira-oauth-done", ok);
    });

    Ok(auth_url)
}

/// Jira-specific post-callback work: fetch the cloud_id + site name and
/// stash access_token / refresh_token / cloud_id / domain in three separate
/// keychain entries.
async fn persist_oauth_result(access_token: &str, refresh_token: Option<&str>) -> Result<()> {
    let (cloud_id, site_name) = fetch_cloud_resource(access_token).await?;
    credential_store::save(KR_TOKEN, "v", access_token)?;
    credential_store::save(KR_CLOUD, "v", &cloud_id)?;
    if let Some(rt) = refresh_token {
        let _ = credential_store::save(KR_REFRESH, "v", rt);
    }
    let sub = site_name.trim_end_matches(".atlassian.net");
    let _ = credential_store::save(KR_DOMAIN, "v", sub);
    Ok(())
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
    let Some(refresh) = credential_store::get(KR_REFRESH, "v")? else {
        return Ok(false);
    };

    let token = match refresh_token(TOKEN_URL, &client_id, None, &refresh, BodyFormat::Json).await {
        Ok(t)  => t,
        Err(e) => {
            tracing::warn!("jira token refresh failed: {e}");
            return Ok(false);
        }
    };

    credential_store::save(KR_TOKEN, "v", &token.access_token)?;
    if let Some(new_rt) = token.refresh_token {
        let _ = credential_store::save(KR_REFRESH, "v", &new_rt);
    }

    tracing::debug!("jira access token refreshed successfully");
    Ok(true)
}
