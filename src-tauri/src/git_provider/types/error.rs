use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unified error type for every `GitProvider` operation.
///
/// `Unsupported` is the canonical "this provider does not implement this
/// feature" error — used by stub modules (releases, issues, webhooks)
/// before they get real implementations.
#[derive(Debug, Error, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
pub enum ProviderError {
    #[error("provider does not support feature: {feature}")]
    Unsupported { feature: String },

    #[error("authentication required")]
    Unauthenticated,

    #[error("authentication failed: {0}")]
    AuthFailed(String),

    #[error("token expired or revoked")]
    TokenExpired,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimited { retry_after_secs: u64 },

    #[error("network error: {0}")]
    Network(String),

    #[error("provider returned {status}: {body}")]
    Http { status: u16, body: String },

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("provider conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<crate::error::AppError> for ProviderError {
    fn from(err: crate::error::AppError) -> Self {
        ProviderError::Internal(err.to_string())
    }
}

impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        ProviderError::Network(err.to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(err: serde_json::Error) -> Self {
        ProviderError::Internal(format!("json: {err}"))
    }
}
