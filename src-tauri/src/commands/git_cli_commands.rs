//! PortableGit download Tauri command.
//!
//! The system-`git` detection/configuration commands (`get_git_status`,
//! `redetect_git`, `verify_git_path`, `set_git_path`, `cancel_git_download`)
//! were migrated to broker handlers in `ipc/corvus/git_cli.rs`.
//! `download_portable_git` stays inline because it takes an `AppHandle` and
//! streams progress via the `arbor://git-download-progress` event — a later
//! emit/seam pass will move it. The `GitCliStatus` DTO + `snapshot_to_status`
//! helper are kept here because they are its return shape.

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::error::AppError;
use crate::git_cli;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCliStatus {
    pub path:      Option<String>,
    pub version:   Option<String>,
    /// "config" | "path" | "portable" | "missing"
    pub source:    Option<String>,
    /// True on platforms where the in-app PortableGit download is implemented.
    pub download_supported: bool,
    /// Default location used by the portable download (shown in the UI).
    pub portable_dir: String,
}

fn snapshot_to_status() -> GitCliStatus {
    let snap = git_cli::snapshot();
    GitCliStatus {
        path:      snap.path.map(|p| p.display().to_string()),
        version:   snap.version,
        source:    snap.source.map(|s| s.to_string()),
        download_supported: git_cli::download_supported(),
        portable_dir: git_cli::portable_dir().display().to_string(),
    }
}

/// Download + extract PortableGit (Windows only) and switch the active path
/// to the bundled binary.  Streams progress via the
/// `arbor://git-download-progress` event so the modal can render a progress bar.
#[tauri::command]
pub async fn download_portable_git(
    app_handle: tauri::AppHandle,
) -> Result<GitCliStatus, AppError> {
    let app_for_progress = app_handle.clone();
    let result = git_cli::download_portable(move |progress| {
        let _ = app_for_progress.emit("arbor://git-download-progress", &progress);
    }).await;

    match result {
        Ok(_path) => {
            // The downloaded portable copy is implicitly the chosen one — but
            // do NOT write `executable_path` to config: the user can still
            // install a system git later and detection should pick that up
            // ahead of the bundled copy.  When they explicitly want the
            // portable one to "win" they set it via Settings → Browse.
            Ok(snapshot_to_status())
        }
        Err(e) => {
            let _ = app_handle.emit("arbor://git-download-progress", &serde_json::json!({
                "stage":   "error",
                "message": e.to_string(),
                "bytes":   0u64,
                "total":   0u64,
            }));
            Err(e)
        }
    }
}
