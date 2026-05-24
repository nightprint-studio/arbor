//! Local error type for the cloud crate.
//!
//! Stays Tauri-agnostic — the host (`src-tauri`) provides
//! `impl From<CloudError> for AppError` so commands can `?`-propagate.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CloudError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CloudError>;
