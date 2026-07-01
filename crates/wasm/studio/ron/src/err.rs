//! Crate-local error shim.
//!
//! The RON backend modules were lifted verbatim from the launcher's
//! `src-tauri/src/ron_studio/`, where they returned `crate::error::Result`
//! (`Result<T, AppError>`). To sever that launcher coupling without
//! rewriting every `AppError::Other(..)` call site, we expose a thin shim:
//! `Result<T>` is now `StudioResult<T>` and `AppError::Other(msg)`
//! constructs a [`StudioError::App`]. The IPC surface is unchanged
//! (`StudioError` stringifies the same way the old `AppError` did).

pub use arbor_studio_types::prelude::StudioError;

pub type Result<T> = std::result::Result<T, StudioError>;

/// Construction shim so the moved modules keep their `AppError::Other(..)`
/// call sites verbatim. `AppError::Other(msg)` yields a [`StudioError::App`].
pub struct AppError;

impl AppError {
    /// Mirror of the launcher's `AppError::Other(String)` — maps to
    /// `StudioError::App`. Named to match the original variant so the
    /// moved bodies need no edits.
    #[allow(non_snake_case)]
    pub fn Other(message: String) -> StudioError {
        StudioError::App(message)
    }
}
