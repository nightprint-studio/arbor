//! Error type for failures originating inside `arbor-core`.
//!
//! Kept intentionally small: this crate only does path resolution and HTTP
//! client construction, so the surface is `Io` + `Http`. The host shell
//! crate provides `impl From<CoreError> for AppError` at the boundary so
//! `?` propagation works everywhere.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, CoreError>;
