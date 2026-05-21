//! Tauri commands for the system `git` executable detection / configuration.
//!
//! The frontend GitSetupModal and Settings → "Git CLI" page drive these.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

use crate::AppState;
use crate::error::AppError;
use crate::config::app_config;
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

/// Read the cached state populated at startup (and after `set_git_path` /
/// `download_portable_git` / `redetect_git`).
#[tauri::command]
pub fn get_git_status() -> Result<GitCliStatus, AppError> {
    Ok(snapshot_to_status())
}

/// Re-run the priority lookup (config override → PATH → portable copy).  Used
/// by the "Auto-detect" button in Settings and after the user installs git
/// system-wide while Arbor is still running.
#[tauri::command]
pub fn redetect_git(state: State<'_, AppState>) -> Result<GitCliStatus, AppError> {
    let configured = {
        let cfg = state.lock_config()?;
        cfg.git.executable_path.clone().filter(|s| !s.is_empty()).map(PathBuf::from)
    };
    git_cli::detect(configured.as_deref());
    Ok(snapshot_to_status())
}

/// Verify that a candidate path is a working git binary (runs `--version`)
/// without persisting it.  Used by the file-picker preview in Settings.
#[tauri::command]
pub fn verify_git_path(path: String) -> Result<String, AppError> {
    let p = PathBuf::from(&path);
    git_cli::verify(&p)
}

/// Persist a new override path.  When `path` is `None`/empty the override is
/// cleared and detection falls back to PATH / portable.  Verifies before
/// saving — returns the resolved status.
#[tauri::command]
pub fn set_git_path(
    state: State<'_, AppState>,
    path: Option<String>,
) -> Result<GitCliStatus, AppError> {
    let trimmed = path.as_deref().map(str::trim).filter(|s| !s.is_empty());

    if let Some(p) = trimmed {
        let candidate = PathBuf::from(p);
        // Verify before persisting so the user gets immediate feedback if
        // the path is wrong, and the on-disk config never references a bad path.
        git_cli::set_path(&candidate, "config")?;
        let mut cfg = state.lock_config()?;
        cfg.git.executable_path = Some(candidate.display().to_string());
        app_config::save(&cfg).map_err(|e| AppError::Other(e.to_string()))?;
    } else {
        // Clear override and re-detect.
        let mut cfg = state.lock_config()?;
        cfg.git.executable_path = None;
        app_config::save(&cfg).map_err(|e| AppError::Other(e.to_string()))?;
        drop(cfg);
        git_cli::clear_override();
    }
    Ok(snapshot_to_status())
}

/// Signal the running PortableGit download to abort at the next checkpoint
/// (between download chunks or 7z entries).  Safe to call when no download
/// is active — flag is reset at the start of every new attempt.
#[tauri::command]
pub fn cancel_git_download() -> Result<(), AppError> {
    git_cli::request_download_cancel();
    Ok(())
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
