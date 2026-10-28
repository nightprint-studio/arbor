//! Credentials → access token, over HTTPS.
//!
//! Two exchanges, both plain JSON/form POSTs: a refresh token for an access token at Google's
//! token endpoint, and — to act as a service account — that access token for the service
//! account's own at the IAM Credentials API. Nothing is cached: a token is minted per connect
//! attempt and handed to the driver, because the connection it authenticates stays valid after the
//! token expires and a reconnect asks again.

use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

use crate::adc::Credentials;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// What Cloud SQL accepts for IAM database login — narrower than `cloud-platform`, and enough.
pub const SQL_LOGIN_SCOPE: &str = "https://www.googleapis.com/auth/sqlservice.login";
/// What the IAM Credentials API itself needs from the *caller's* token.
const CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const IAM_CREDENTIALS: &str = "https://iamcredentials.googleapis.com/v1/projects/-/serviceAccounts";

/// A client that cannot hang a connect forever: a laptop off the network must fail in seconds,
/// not leave the Connect button spinning.
pub fn client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("cannot start the HTTP client: {e}"))
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Deserialize)]
struct ImpersonationReply {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    error: Option<GoogleApiError>,
}

#[derive(Deserialize)]
struct GoogleApiError {
    message: Option<String>,
    status: Option<String>,
}

/// An access token for the Cloud SQL login scope, as `creds` (or the service account they
/// impersonate) — and, when `impersonate` is given, as that service account instead.
pub async fn sql_login_token(
    http: &Client,
    creds: &Credentials,
    impersonate: Option<&str>,
) -> Result<String, String> {
    match (creds, impersonate) {
        // Impersonation written into the file, no override on the connection.
        (Credentials::Impersonated { url, source }, None) => {
            let source_token = Box::pin(base_token(http, source, CLOUD_PLATFORM_SCOPE)).await?;
            impersonate_at(http, url, &source_token).await
        }
        // The connection's own override wins: act as that account, authenticating as the user.
        (_, Some(account)) => {
            let source_token = Box::pin(base_token(http, creds, CLOUD_PLATFORM_SCOPE)).await?;
            let url = format!("{IAM_CREDENTIALS}/{}:generateAccessToken", account.trim());
            impersonate_at(http, &url, &source_token).await
        }
        (creds, None) => base_token(http, creds, SQL_LOGIN_SCOPE).await,
    }
}

/// A token for `scope` from a credentials value that is not itself an impersonation.
async fn base_token(http: &Client, creds: &Credentials, scope: &str) -> Result<String, String> {
    match creds {
        Credentials::AuthorizedUser { client_id, client_secret, refresh_token } => {
            // The refresh grant may ask for less than was consented to, never more; the scope is
            // sent so the token is as narrow as the use.
            let reply = http
                .post(TOKEN_URL)
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", client_id),
                    ("client_secret", client_secret),
                    ("refresh_token", refresh_token),
                    ("scope", scope),
                ])
                .send()
                .await
                .map_err(network_error)?;
            let status = reply.status();
            let body: TokenReply = reply.json().await.map_err(|e| format!("Google answered with something unreadable ({status}): {e}"))?;
            match body.access_token.as_deref().filter(|t| !t.is_empty()) {
                Some(token) => Ok(token.to_string()),
                None => Err(token_error(&body)),
            }
        }
        Credentials::ServiceAccount { client_email, private_key, private_key_id, token_uri } => {
            let assertion = sign_assertion(client_email, private_key, private_key_id, token_uri, scope)?;
            let reply = http
                .post(token_uri)
                .form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", assertion.as_str())])
                .send()
                .await
                .map_err(network_error)?;
            let status = reply.status();
            let body: TokenReply = reply.json().await.map_err(|e| format!("Google answered with something unreadable ({status}): {e}"))?;
            match body.access_token.as_deref().filter(|t| !t.is_empty()) {
                Some(token) => Ok(token.to_string()),
                None => Err(token_error(&body)),
            }
        }
        Credentials::Impersonated { url, source } => {
            let source_token = Box::pin(base_token(http, source, CLOUD_PLATFORM_SCOPE)).await?;
            impersonate_at(http, url, &source_token).await
        }
    }
}

/// The signed JWT a service account trades for a token: header, claims, RS256 signature.
fn sign_assertion(email: &str, pem: &str, key_id: &str, audience: &str, scope: &str) -> Result<String, String> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use rsa::pkcs8::DecodePrivateKey;
    use rsa::signature::{SignatureEncoding, Signer};

    let key = rsa::RsaPrivateKey::from_pkcs8_pem(pem)
        .map_err(|_| "the private key in the service-account file cannot be read (is it truncated or re-encoded?)".to_string())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "the system clock is before 1970".to_string())?
        .as_secs();
    let mut header = serde_json::json!({ "alg": "RS256", "typ": "JWT" });
    if !key_id.is_empty() {
        header["kid"] = serde_json::json!(key_id);
    }
    // An hour is the maximum Google accepts; the token minted from it is used at once.
    let claims = serde_json::json!({ "iss": email, "sub": email, "scope": scope, "aud": audience, "iat": now, "exp": now + 3600 });
    let signing_input = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
    let signature = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(key).sign(signing_input.as_bytes());
    Ok(format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes())))
}

async fn impersonate_at(http: &Client, url: &str, source_token: &str) -> Result<String, String> {
    let reply = http
        .post(url)
        .bearer_auth(source_token)
        .json(&serde_json::json!({ "scope": [SQL_LOGIN_SCOPE] }))
        .send()
        .await
        .map_err(network_error)?;
    let status = reply.status();
    let body: ImpersonationReply =
        reply.json().await.map_err(|e| format!("Google answered with something unreadable ({status}): {e}"))?;
    if let Some(token) = body.access_token.filter(|t| !t.is_empty()) {
        return Ok(token);
    }
    Err(impersonation_error(status.as_u16(), body.error, url))
}

fn network_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "Google did not answer in time — check the network or proxy".to_string()
    } else if e.is_connect() {
        "cannot reach Google — check the network or proxy".to_string()
    } else {
        format!("the request to Google failed: {e}")
    }
}

/// The token endpoint's complaint, with the common ones turned into what to do about them.
fn token_error(body: &TokenReply) -> String {
    match body.error.as_deref() {
        Some("invalid_grant") => "Google rejected the stored sign-in (revoked, expired, or password changed) — \
                                  run `gcloud auth application-default login` again"
            .to_string(),
        Some("invalid_scope") => "the stored sign-in was not granted access to Cloud SQL login — \
                                  run `gcloud auth application-default login` again"
            .to_string(),
        Some("invalid_client") | Some("unauthorized_client") => {
            "Google does not accept the OAuth client in the credentials file — run `gcloud auth application-default login` again".to_string()
        }
        Some(other) => match body.error_description.as_deref() {
            Some(d) if !d.is_empty() => format!("Google refused the sign-in: {other} ({d})"),
            _ => format!("Google refused the sign-in: {other}"),
        },
        None => "Google returned no access token".to_string(),
    }
}

fn impersonation_error(status: u16, error: Option<GoogleApiError>, url: &str) -> String {
    let account = url
        .rsplit('/')
        .next()
        .and_then(|s| s.split(':').next())
        .unwrap_or("the service account");
    match (status, error.as_ref().and_then(|e| e.status.as_deref())) {
        (403, _) | (_, Some("PERMISSION_DENIED")) => format!(
            "you may not act as {account} — it needs the role Service Account Token Creator \
             (roles/iam.serviceAccountTokenCreator) for your account, and the IAM Credentials API enabled"
        ),
        (404, _) | (_, Some("NOT_FOUND")) => format!("{account} does not exist"),
        _ => match error.and_then(|e| e.message) {
            Some(m) => format!("impersonating {account} failed: {m}"),
            None => format!("impersonating {account} failed (HTTP {status})"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(error: Option<&str>, description: Option<&str>) -> TokenReply {
        TokenReply {
            access_token: None,
            error: error.map(String::from),
            error_description: description.map(String::from),
        }
    }

    #[test]
    fn a_revoked_grant_says_to_sign_in_again() {
        assert!(token_error(&reply(Some("invalid_grant"), None)).contains("gcloud auth application-default login"));
    }

    #[test]
    fn an_unknown_error_keeps_its_description() {
        let m = token_error(&reply(Some("weird"), Some("because")));
        assert!(m.contains("weird") && m.contains("because"));
    }

    #[test]
    fn a_forbidden_impersonation_names_the_role_and_the_account() {
        let m = impersonation_error(
            403,
            None,
            "https://iamcredentials.googleapis.com/v1/projects/-/serviceAccounts/sa@p.iam.gserviceaccount.com:generateAccessToken",
        );
        assert!(m.contains("sa@p.iam.gserviceaccount.com") && m.contains("serviceAccountTokenCreator"));
    }
}
