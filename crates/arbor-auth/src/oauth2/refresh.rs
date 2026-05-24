//! OAuth2 `refresh_token` grant — pure HTTP, no UI / browser.

use crate::error::{AuthError, Result};
use crate::types::{BodyFormat, TokenResponse, TokenWire};

/// Exchange a refresh token for a fresh access token.
///
/// `format` follows the same convention as
/// [`crate::oauth2::InstalledAppFlow::token_request_format`] — match the
/// provider's documented body format.
pub async fn refresh_token(
    token_url:     &str,
    client_id:     &str,
    client_secret: Option<&str>,
    refresh_token: &str,
    format:        BodyFormat,
) -> Result<TokenResponse> {
    let client = reqwest::Client::new();
    // Accept JSON is harmless for providers that always reply JSON and
    // required for GitHub which otherwise defaults to form-encoded text.
    let req = client.post(token_url).header("Accept", "application/json");

    let req = match format {
        BodyFormat::Form => {
            let mut form: Vec<(&str, &str)> = vec![
                ("grant_type",    "refresh_token"),
                ("refresh_token", refresh_token),
                ("client_id",     client_id),
            ];
            if let Some(secret) = client_secret {
                if !secret.is_empty() { form.push(("client_secret", secret)); }
            }
            req.form(&form)
        }
        BodyFormat::Json => {
            let mut body = serde_json::json!({
                "grant_type":    "refresh_token",
                "refresh_token": refresh_token,
                "client_id":     client_id,
            });
            if let Some(secret) = client_secret {
                if !secret.is_empty() {
                    body.as_object_mut().unwrap()
                        .insert("client_secret".into(), serde_json::Value::String(secret.into()));
                }
            }
            req.json(&body)
        }
    };

    let resp = req.send().await
        .map_err(|e| AuthError::Http(format!("refresh request: {e}")))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(AuthError::HttpStatus { status: status.as_u16(), body });
    }
    let raw: serde_json::Value = resp.json().await
        .map_err(|e| AuthError::Http(format!("refresh response decode: {e}")))?;
    let wire: TokenWire = serde_json::from_value(raw.clone())
        .map_err(|e| AuthError::Http(format!("refresh response shape: {e}")))?;
    Ok(TokenResponse {
        access_token:  wire.access_token,
        refresh_token: wire.refresh_token,
        expires_in:    wire.expires_in,
        raw,
    })
}
