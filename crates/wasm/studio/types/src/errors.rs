//! Studio format errors — surfaces the small set of failures the
//! generic studio commands need to distinguish from format-specific
//! parse/IO errors.
//!
//! Stage 1 of the studio crate extraction decoupled this from the
//! launcher's `crate::error::AppError`: the `App` variant now carries a
//! plain `String`. Backends map their own parse/IO failures into it via
//! `StudioError::backend(..)` / `.to_string()`. The launcher's `to_ipc`
//! still stringifies, so the IPC surface is identical.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StudioError {
    #[error("unknown studio format `{0}`")]
    UnknownFormat(String),

    #[error("studio capability `{capability}` not supported by format `{format}`")]
    Unsupported { format: &'static str, capability: &'static str },

    /// Backend / IO / parse failure, already stringified. Replaces the
    /// former `App(#[from] AppError)` coupling to the launcher.
    #[error("{0}")]
    App(String),
}

impl StudioError {
    pub fn unsupported(format: &'static str, capability: &'static str) -> Self {
        StudioError::Unsupported { format, capability }
    }

    /// Construct an `App` (backend) error from anything string-like.
    pub fn backend(message: impl Into<String>) -> Self {
        StudioError::App(message.into())
    }
}

pub type StudioResult<T> = std::result::Result<T, StudioError>;

/// Convert a `StudioResult<T>` to the `Result<T, String>` shape the
/// Tauri command layer wants. Keeps the error message readable on the
/// frontend without surfacing the enum machinery.
pub fn to_ipc<T>(r: StudioResult<T>) -> std::result::Result<T, String> {
    r.map_err(|e| e.to_string())
}
