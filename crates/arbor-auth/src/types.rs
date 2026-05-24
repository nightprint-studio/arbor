use serde::Deserialize;

/// How the token / device-code endpoint expects its request body.
///
/// Google + GitHub use `Form`; Jira + Linear use `Json`. The endpoint
/// usually documents this — pick the matching variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyFormat {
    /// `application/x-www-form-urlencoded` (the OAuth2 spec default).
    Form,
    /// `application/json`.
    Json,
}

/// Result of a successful token exchange or refresh.
///
/// `raw` carries the full decoded response so callers can read
/// provider-specific extras (Atlassian's `scope`, Google's `id_token`,
/// etc.) without the crate needing to know about them.
#[derive(Debug, Clone)]
pub struct TokenResponse {
    pub access_token:  String,
    pub refresh_token: Option<String>,
    /// Seconds until `access_token` expires, when the provider reports it.
    pub expires_in:    Option<i64>,
    pub raw:           serde_json::Value,
}

/// Wire shape used to decode the JSON body — internal, never exposed.
#[derive(Debug, Deserialize)]
pub(crate) struct TokenWire {
    pub(crate) access_token:  String,
    #[serde(default)]
    pub(crate) refresh_token: Option<String>,
    #[serde(default)]
    pub(crate) expires_in:    Option<i64>,
}

/// Response from the device-code endpoint (RFC 8628 §3.2).
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    /// Opaque code the client polls with.
    pub device_code:      String,
    /// Short, human-friendly code the user types at the verification URI.
    pub user_code:        String,
    /// URI where the user enters the `user_code` (e.g. https://github.com/login/device).
    pub verification_uri: String,
    /// Convenience URI that already encodes the user_code (when supplied).
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    /// Lifetime of the device + user codes in seconds.
    #[serde(default = "default_expires")]
    pub expires_in: u64,
    /// Minimum polling interval in seconds (RFC 8628 §3.5).
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_expires() -> u64 { 900 }   // 15 min — spec recommendation
fn default_interval() -> u64 { 5 }
