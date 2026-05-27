//! Error type for failures originating inside `arbor-plugin-core`.
//!
//! Mapped to the host shell's `AppError` via a `From<PluginCoreError>` impl at
//! the boundary. The variants intentionally mirror the legacy
//! `AppError::Plugin` / `AppError::Other` / `AppError::Io` shape so existing
//! call sites — and the on-the-wire error strings shown to plugin authors —
//! stay unchanged through the migration.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginCoreError {
    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PluginCoreError>;
