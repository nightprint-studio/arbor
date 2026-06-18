//! `git_cli` domain — system-`git` detection/configuration handlers routed
//! through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The
//! domain's logic already lives in the reusable, global-state shell module
//! [`crate::git_cli`] (process detection, the portable-git cache, the cancel
//! flag), so handlers **delegate straight to it** — there is no Tauri-free
//! `corvus-git` extraction here (the work is global mutable state + subprocess
//! shelling, not pure git plumbing). Behavior (locks held, config save, errors)
//! is byte-identical.
//!
//! `get_git_status` / `verify_git_path` / `cancel_git_download` never touched
//! `AppState`, but the handler macro requires a context first arg, so they take
//! `_state: &AppState` and ignore it — same as the original parameter-less
//! commands.
//!
//! `download_portable_git` streams progress via the `arbor://git-download-progress`
//! event through the backend [`EventSink`] (`state.event_sink()`) instead of an
//! `AppHandle` — Model-D-safe: in-process it forwards to the Tauri emitter,
//! post-split it crosses the IPC event channel. It is `async` (network download)
//! so the generic `rpc` command awaits it on the runtime.
//!
//! No hooks fire in this domain.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::app_config;
use crate::error::AppError;
use crate::git_cli;
use crate::ipc::corvus;
use crate::AppState;

/// Mirror of the command module's status DTO — same fields, same serde shape, so
/// the FE `GitCliStatus` decodes identically whether the call routes here or
/// through the legacy `#[tauri::command]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCliStatus {
    pub path:    Option<String>,
    pub version: Option<String>,
    /// "config" | "path" | "portable" | "missing"
    pub source:  Option<String>,
    /// True on platforms where the in-app PortableGit download is implemented.
    pub download_supported: bool,
    /// Default location used by the portable download (shown in the UI).
    pub portable_dir: String,
}

fn snapshot_to_status() -> GitCliStatus {
    let snap = git_cli::snapshot();
    GitCliStatus {
        path:    snap.path.map(|p| p.display().to_string()),
        version: snap.version,
        source:  snap.source.map(|s| s.to_string()),
        download_supported: git_cli::download_supported(),
        portable_dir: git_cli::portable_dir().display().to_string(),
    }
}

/// Read the cached state populated at startup (and after `set_git_path` /
/// `download_portable_git` / `redetect_git`).
#[corvus::handler]
fn get_git_status(_state: &AppState) -> Result<GitCliStatus, AppError> {
    Ok(snapshot_to_status())
}

/// Re-run the priority lookup (config override → PATH → portable copy).
#[corvus::handler]
fn redetect_git(state: &AppState) -> Result<GitCliStatus, AppError> {
    let configured = {
        let cfg = state.lock_config()?;
        cfg.git.executable_path.clone().filter(|s| !s.is_empty()).map(PathBuf::from)
    };
    git_cli::detect(configured.as_deref());
    Ok(snapshot_to_status())
}

/// Verify that a candidate path is a working git binary (runs `--version`)
/// without persisting it.
#[corvus::handler]
fn verify_git_path(_state: &AppState, path: String) -> Result<String, AppError> {
    let p = PathBuf::from(&path);
    git_cli::verify(&p)
}

/// Persist a new override path. When `path` is `None`/empty the override is
/// cleared and detection falls back to PATH / portable. Verifies before saving —
/// returns the resolved status.
#[corvus::handler]
fn set_git_path(state: &AppState, path: Option<String>) -> Result<GitCliStatus, AppError> {
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
/// (between download chunks or 7z entries). Safe to call when no download is
/// active — flag is reset at the start of every new attempt.
#[corvus::handler]
fn cancel_git_download(_state: &AppState) -> Result<(), AppError> {
    git_cli::request_download_cancel();
    Ok(())
}

/// Download + extract PortableGit (Windows only) and switch the active path to
/// the bundled binary. Streams progress via the `arbor://git-download-progress`
/// event so the modal can render a progress bar. Does **not** persist the path
/// to config: the user can still install a system git later and detection should
/// prefer it over the bundled copy (they pin the portable one via Settings →
/// Browse if they want it to win).
#[corvus::handler]
async fn download_portable_git(state: &AppState) -> Result<GitCliStatus, AppError> {
    let sink = state.event_sink();
    let sink_for_progress = sink.clone();
    let result = git_cli::download_portable(move |progress| {
        if let Some(sink) = &sink_for_progress {
            sink.emit(
                "arbor://git-download-progress",
                serde_json::to_value(&progress).unwrap_or(serde_json::Value::Null),
            );
        }
    })
    .await;

    match result {
        Ok(_path) => Ok(snapshot_to_status()),
        Err(e) => {
            if let Some(sink) = &sink {
                sink.emit("arbor://git-download-progress", serde_json::json!({
                    "stage":   "error",
                    "message": e.to_string(),
                    "bytes":   0u64,
                    "total":   0u64,
                }));
            }
            Err(e)
        }
    }
}
