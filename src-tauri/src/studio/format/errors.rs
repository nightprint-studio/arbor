//! Studio format errors — surfaces the small set of failures the
//! generic studio commands need to distinguish from format-specific
//! parse/IO errors (which are domain noise that the per-format impl
//! already wraps in `AppError`).

use thiserror::Error;

use crate::error::AppError;

#[derive(Debug, Error)]
pub enum StudioError {
    #[error("unknown studio format `{0}`")]
    UnknownFormat(String),

    #[error("studio capability `{capability}` not supported by format `{format}`")]
    Unsupported { format: &'static str, capability: &'static str },

    #[error(transparent)]
    App(#[from] AppError),
}

impl StudioError {
    pub fn unsupported(format: &'static str, capability: &'static str) -> Self {
        StudioError::Unsupported { format, capability }
    }
}

pub type StudioResult<T> = std::result::Result<T, StudioError>;

/// Convert a `StudioResult<T>` to the `Result<T, String>` shape the
/// Tauri command layer wants. Keeps the error message readable on the
/// frontend without surfacing the enum machinery.
pub fn to_ipc<T>(r: StudioResult<T>) -> std::result::Result<T, String> {
    r.map_err(|e| e.to_string())
}
