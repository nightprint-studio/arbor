//! GCS auth resolution.
//!
//! Produces something `opendal::services::Gcs` can accept:
//!   * [`Resolved::Credential`] — full service-account JSON. opendal will
//!     JWT-sign on its own.
//!   * [`Resolved::Token`] — a pre-fetched OAuth2 bearer token (used when
//!     we have a user refresh token, the `gcloud` CLI, or an authorized-user
//!     ADC file).
//!
//! Five inputs map onto the two outputs:
//!
//!   GcsAuth::SaFile{path}      → read file → Credential(json)
//!   GcsAuth::SaInline{ref}     → keyring  → Credential(json)
//!   GcsAuth::Adc               → discover ADC file →
//!                                  if service_account → Credential(json)
//!                                  if authorized_user → refresh → Token
//!   GcsAuth::GcloudCli         → spawn `gcloud auth print-access-token` → Token
//!   GcsAuth::Oauth{ref}        → keyring blob → refresh → Token
//!
//! For the OAuth refresh path see `cloud/oauth_google.rs` (still in
//! `src-tauri` until Phase B) — `Resolved::Oauth` calls back into it.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;

use crate::error::{CloudError, Result};
use crate::secrets;
use crate::types::GcsAuth;

/// What auth_gcs resolves to.
pub enum Resolved {
    /// Full SA JSON content — pass to opendal as `credential`.
    Credential { json: String, identity: Option<String> },
    /// Pre-fetched OAuth2 bearer token — pass to opendal as `token`.
    Token     { access_token: String, identity: Option<String> },
}

impl Resolved {
    pub fn identity(&self) -> Option<&str> {
        match self {
            Self::Credential { identity, .. } => identity.as_deref(),
            Self::Token      { identity, .. } => identity.as_deref(),
        }
    }
}

/// Callback used for the `GcsAuth::Oauth` branch: takes a `StoredOAuth`
/// blob, performs the Google refresh-token exchange, returns the refreshed
/// access token + expiry. Implemented by `src-tauri/src/cloud/oauth_google.rs`
/// during Phase A; folded into this crate during Phase B.
///
/// `StoredOAuth` is passed by value (cloned) to avoid the higher-ranked
/// lifetime ergonomics of closures that borrow their argument. Refresh runs
/// at most once per ~50 minutes per connection so the extra clone of five
/// `String` fields is irrelevant.
pub type OAuthRefresher = std::sync::Arc<
    dyn Fn(StoredOAuth)
        -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<RefreshedToken>> + Send>>
        + Send + Sync,
>;

#[derive(Debug, Clone)]
pub struct RefreshedToken {
    pub access_token:    String,
    pub expires_in_secs: u64,
}

/// Process-wide OAuth refresher. The host (src-tauri) installs this once at
/// startup; `resolve(GcsAuth::Oauth { .. })` reads it back. Avoids threading
/// the closure through every call site (operator::build, ops::*, transfer::*).
static OAUTH_REFRESHER: std::sync::OnceLock<OAuthRefresher> = std::sync::OnceLock::new();

/// Install the OAuth refresher. Idempotent — only the first call wins.
/// Calling this is mandatory if any connection uses `GcsAuth::Oauth`.
pub fn install_oauth_refresher(refresher: OAuthRefresher) {
    let _ = OAUTH_REFRESHER.set(refresher);
}

/// Resolve `auth` into something opendal can use.
pub async fn resolve(auth: &GcsAuth) -> Result<Resolved> {
    match auth {
        GcsAuth::SaFile { path } => {
            let json = std::fs::read_to_string(path)
                .map_err(|e| CloudError::AuthFailed(format!("read {path}: {e}")))?;
            let identity = sa_client_email(&json);
            Ok(Resolved::Credential { json, identity })
        }
        GcsAuth::SaInline { secret_ref } => {
            let json = secrets::get(secret_ref)?
                .ok_or_else(|| CloudError::AuthFailed(
                    format!("inline SA secret not found in keyring: {secret_ref}")
                ))?;
            let identity = sa_client_email(&json);
            Ok(Resolved::Credential { json, identity })
        }
        GcsAuth::Adc => {
            let path = discover_adc_path()
                .ok_or_else(|| CloudError::AuthFailed(
                    "ADC not found: set GOOGLE_APPLICATION_CREDENTIALS or run \
                     `gcloud auth application-default login`".into()
                ))?;
            let json = std::fs::read_to_string(&path)
                .map_err(|e| CloudError::AuthFailed(format!("read ADC {}: {e}", path.display())))?;
            // The ADC file is either a service-account or an authorized-user
            // credential.  The two are distinguished by a "type" field.
            match adc_type(&json) {
                Some("service_account") => {
                    let identity = sa_client_email(&json);
                    Ok(Resolved::Credential { json, identity })
                }
                Some("authorized_user") => {
                    let access = refresh_authorized_user(&json).await?;
                    Ok(Resolved::Token { access_token: access, identity: None })
                }
                other => Err(CloudError::AuthFailed(format!(
                    "ADC file has unsupported type: {:?}", other.unwrap_or("<none>")
                ))),
            }
        }
        GcsAuth::GcloudCli => {
            let access = gcloud_print_access_token().await?;
            Ok(Resolved::Token { access_token: access, identity: None })
        }
        GcsAuth::Oauth { secret_ref } => {
            let refresher = OAUTH_REFRESHER.get().ok_or_else(|| CloudError::AuthFailed(
                "GcsAuth::Oauth requires arbor_cloud::auth_gcs::install_oauth_refresher \
                 to have been called at host startup".into()
            ))?;
            let blob = secrets::get(secret_ref)?
                .ok_or_else(|| CloudError::AuthFailed(format!(
                    "OAuth refresh token not found in keyring: {secret_ref} \
                     (re-run the authorization flow)"
                )))?;
            let stored: StoredOAuth = serde_json::from_str(&blob)
                .map_err(|e| CloudError::AuthFailed(format!("malformed OAuth blob: {e}")))?;
            // If the cached access token still has >=60s of life, reuse it.
            if let (Some(tok), Some(exp)) = (&stored.access_token, stored.expires_at) {
                if exp.saturating_sub(now_secs()) > 60 {
                    return Ok(Resolved::Token { access_token: tok.clone(), identity: None });
                }
            }
            let refreshed = refresher(stored.clone()).await?;
            // Persist the refreshed access token / expiry so future calls reuse it.
            let to_store = StoredOAuth {
                refresh_token: stored.refresh_token,
                access_token:  Some(refreshed.access_token.clone()),
                expires_at:    Some(now_secs() + refreshed.expires_in_secs.saturating_sub(30)),
                client_id:     stored.client_id,
                client_secret: stored.client_secret,
            };
            let updated = serde_json::to_string(&to_store)
                .map_err(|e| CloudError::AuthFailed(e.to_string()))?;
            secrets::set(secret_ref, &updated)?;
            Ok(Resolved::Token { access_token: refreshed.access_token, identity: None })
        }
    }
}

// ── ADC discovery ───────────────────────────────────────────────────────────

fn discover_adc_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()
            .map(|p| PathBuf::from(p).join("gcloud").join("application_default_credentials.json"))
    } else {
        dirs::home_dir().map(|h| h.join(".config").join("gcloud").join("application_default_credentials.json"))
    };
    base.and_then(|p| if p.exists() { Some(p) } else { None })
}

fn adc_type(json: &str) -> Option<&'static str> {
    // Avoid pulling a full serde_json::Value just to read one field.
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match v.get("type")?.as_str()? {
        "service_account" => Some("service_account"),
        "authorized_user" => Some("authorized_user"),
        _ => None,
    }
}

fn sa_client_email(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("client_email")?.as_str().map(|s| s.to_string())
}

// ── Authorized-user refresh ─────────────────────────────────────────────────

/// Shape of `~/.config/gcloud/application_default_credentials.json` when it
/// was created by `gcloud auth application-default login` (user creds).
#[derive(Debug, Deserialize)]
struct AdcAuthorizedUser {
    client_id:     String,
    client_secret: String,
    refresh_token: String,
}

async fn refresh_authorized_user(json: &str) -> Result<String> {
    let cfg: AdcAuthorizedUser = serde_json::from_str(json)
        .map_err(|e| CloudError::AuthFailed(format!("malformed authorized_user ADC: {e}")))?;

    let resp = reqwest::Client::new()
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type",    "refresh_token"),
            ("refresh_token", cfg.refresh_token.as_str()),
            ("client_id",     cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
        ])
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| CloudError::AuthFailed(format!("ADC refresh request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(CloudError::AuthFailed(format!("ADC refresh HTTP {status}: {body}")));
    }
    #[derive(Deserialize)]
    struct R { access_token: String }
    let r: R = resp.json().await
        .map_err(|e| CloudError::AuthFailed(format!("ADC refresh decode: {e}")))?;
    Ok(r.access_token)
}

// ── gcloud CLI ──────────────────────────────────────────────────────────────

async fn gcloud_print_access_token() -> Result<String> {
    // Spawn on a blocking task — `Command::output` is sync and ~200ms of
    // startup cost is enough to matter on Windows.
    let out = tokio::task::spawn_blocking(|| {
        use std::process::Command;
        use arbor_process_ext::NoWindowExt;
        Command::new("gcloud")
            .args(["auth", "print-access-token"])
            .no_window()
            .output()
    })
    .await
    .map_err(|e| CloudError::AuthFailed(format!("gcloud spawn join: {e}")))?
    .map_err(|e| CloudError::AuthFailed(format!(
        "gcloud not found or failed to launch: {e} — install Google Cloud SDK or pick a different auth method"
    )))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(CloudError::AuthFailed(format!(
            "gcloud auth print-access-token failed: {}",
            stderr.trim()
        )));
    }
    let tok = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if tok.is_empty() {
        return Err(CloudError::AuthFailed(
            "gcloud returned an empty token (re-run `gcloud auth login`)".into()
        ));
    }
    Ok(tok)
}

// ── Stored OAuth blob (re-used by oauth_google.rs) ──────────────────────────

/// Serialised under `secret_ref` in keyring. `access_token` + `expires_at`
/// are best-effort cache fields; only `refresh_token` is load-bearing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredOAuth {
    pub refresh_token: String,
    #[serde(default)]
    pub access_token:  Option<String>,
    /// Unix epoch seconds when `access_token` expires.
    #[serde(default)]
    pub expires_at:    Option<u64>,
    /// OAuth client_id used to obtain this refresh token. Stored so refresh
    /// continues to work even if the plugin setting changes underneath us.
    #[serde(default)]
    pub client_id:     Option<String>,
    /// Optional client secret for installed-app flow (Google issues one even
    /// though installed apps are public — pass it if known).
    #[serde(default)]
    pub client_secret: Option<String>,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
