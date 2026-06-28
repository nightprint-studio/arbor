//! The crate's error type.
//!
//! Mirrors the three shell `AppError` variants this module produces (`Other` /
//! `Cancelled` / `Unsupported`) **Display-for-Display**, so the shell's
//! `From<GitCliError> for AppError` is a lossless variant remap and a future OOP
//! handler returning `e.to_string()` yields a byte-identical wire string.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitCliError {
    #[error("{0}")]
    Other(String),

    #[error("Operation cancelled")]
    Cancelled,

    #[error("Unsupported: {0}")]
    Unsupported(String),
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, GitCliError>;
