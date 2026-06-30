//! `git_cli` domain — system-`git` detection/configuration, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → wire method names) as the shell's
//! in-process copy (`crate::ipc::corvus::git_cli`), delegating straight to the
//! reusable [`corvus_git_cli`] crate. There is no Tauri-free extraction beyond
//! that crate: the domain's logic is process-global mutable state (the detection
//! snapshot, the portable-git cache, the cancel flag) plus subprocess shelling,
//! not pure git plumbing. The detection state is the crate's **process-global**:
//! the shell and this headless backend each own their own instance and
//! **self-detect** — corvus-be re-runs the priority lookup against its own state.
//!
//! `get_git_status` / `verify_git_path` / `cancel_git_download` never touch the
//! [`CorvusState`], but the handler macro requires a context first arg, so they
//! take `_state` and ignore it — same as the original parameter-less commands.
//!
//! **Config persistence is shell-side.** The keyring/profile-aware
//! `config.toml` lives in the shell, so corvus-be never reads or writes a config
//! file: `set_git_path` persists the `[git] executable_path` override by calling
//! the shell's `__persist_git_path` host method over the reverse channel.
//! `redetect_git` reads the *already-pushed* `"git"` config section (via
//! [`CorvusState::config`]) to recover the configured override before
//! re-detecting.
//!
//! `download_portable_git` streams progress via the `arbor://git-download-progress`
//! event through the backend [`EventSink`] (`state.event_sink()`). It is `async`
//! (network download) so the generic `rpc` command awaits it on the runtime.
//!
//! No hooks fire in this domain.

use std::path::PathBuf;
use std::sync::Arc;

use arbor_ipc::prelude::EventSink;
use corvus_core::prelude::CorvusState;
use serde::{Deserialize, Serialize};

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
    let snap = corvus_git_cli::snapshot();
    GitCliStatus {
        path:    snap.path.map(|p| p.display().to_string()),
        version: snap.version,
        source:  snap.source.map(|s| s.to_string()),
        download_supported: corvus_git_cli::download_supported(),
        portable_dir: corvus_git_cli::portable_dir().display().to_string(),
    }
}

/// Read the cached state populated at startup (and after `set_git_path` /
/// `download_portable_git` / `redetect_git`).
#[arbor_rpc::handler]
fn get_git_status(_state: &CorvusState) -> Result<GitCliStatus, String> {
    Ok(snapshot_to_status())
}

/// Re-run the priority lookup (config override → PATH → portable copy). The
/// override comes from the shell-pushed `"git"` config section, not a local
/// config file.
#[arbor_rpc::handler]
fn redetect_git(state: &CorvusState) -> Result<GitCliStatus, String> {
    let configured: Option<PathBuf> = state
        .config("git")
        .as_ref()
        .and_then(|cfg| cfg.get("executable_path"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    corvus_git_cli::detect(configured.as_deref());
    Ok(snapshot_to_status())
}

/// Verify that a candidate path is a working git binary (runs `--version`)
/// without persisting it.
#[arbor_rpc::handler]
fn verify_git_path(_state: &CorvusState, path: String) -> Result<String, String> {
    corvus_git_cli::verify(&PathBuf::from(&path)).map_err(|e| e.to_string())
}

/// Persist a new override path. When `path` is `None`/empty the override is
/// cleared and detection falls back to PATH / portable. Verifies before saving —
/// returns the resolved status. Persistence of the `[git] executable_path`
/// override is shell-side, via the `__persist_git_path` host method.
#[arbor_rpc::handler]
fn set_git_path(state: &CorvusState, path: Option<String>) -> Result<GitCliStatus, String> {
    let trimmed = path.as_deref().map(str::trim).filter(|s| !s.is_empty());

    if let Some(p) = trimmed {
        // Verify before persisting so the user gets immediate feedback if the
        // path is wrong, and the on-disk config never references a bad path.
        corvus_git_cli::set_path(&PathBuf::from(p), "config").map_err(|e| e.to_string())?;
        state.host_call("__persist_git_path", serde_json::json!({ "path": p }))?;
    } else {
        // Clear override and re-detect.
        state.host_call(
            "__persist_git_path",
            serde_json::json!({ "path": serde_json::Value::Null }),
        )?;
        corvus_git_cli::clear_override();
    }
    Ok(snapshot_to_status())
}

/// Signal the running PortableGit download to abort at the next checkpoint
/// (between download chunks or 7z entries). Safe to call when no download is
/// active — flag is reset at the start of every new attempt.
#[arbor_rpc::handler]
fn cancel_git_download(_state: &CorvusState) -> Result<(), String> {
    corvus_git_cli::request_download_cancel();
    Ok(())
}

/// Download + extract PortableGit (Windows only) and switch the active path to
/// the bundled binary. Streams progress via the `arbor://git-download-progress`
/// event so the modal can render a progress bar. Does **not** persist the path
/// to config: the user can still install a system git later and detection should
/// prefer it over the bundled copy (they pin the portable one via Settings →
/// Browse if they want it to win).
#[arbor_rpc::handler]
async fn download_portable_git(state: &CorvusState) -> Result<GitCliStatus, String> {
    let sink: Arc<dyn EventSink> = state.event_sink();
    let sink_for_progress = Arc::clone(&sink);
    let result = corvus_git_cli::download_portable(move |progress| {
        sink_for_progress.emit(
            "arbor://git-download-progress",
            serde_json::to_value(&progress).unwrap_or(serde_json::Value::Null),
        );
    })
    .await;

    match result {
        Ok(_path) => Ok(snapshot_to_status()),
        Err(e) => {
            sink.emit(
                "arbor://git-download-progress",
                serde_json::json!({
                    "stage":   "error",
                    "message": e.to_string(),
                    "bytes":   0u64,
                    "total":   0u64,
                }),
            );
            Err(e.to_string())
        }
    }
}
