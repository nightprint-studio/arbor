//! Git awareness for the built-in File Explorer — shell-side glue.
//!
//! The status overlays, inline light actions (stage / unstage / discard /
//! ignore), branch list/switch, and remote-url lookup all moved to the corvus
//! broker, now served out-of-process by `corvus-be`
//! (`crates/corvus/be/src/fs_git.rs`), which carries the shared per-repo status
//! cache with them. The only command left here is the
//! heavy-action delegation, which is genuine shell glue: it needs an
//! `AppHandle` to bring the main window forward (a WebView2 main-thread
//! operation) and emit a targeted event to it.

use git2::Repository;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;

/// Bring the main Arbor window forward and ask it to open the repo containing
/// `path`. The heavy git operations (diff / log / blame / commit) live in the
/// main window's full git UI; the explorer just delegates to it.
#[tauri::command]
pub fn fs_open_in_arbor(app: AppHandle, path: String) -> Result<(), AppError> {
    let repo = Repository::discover(&path)
        .map_err(|_| AppError::Other("not inside a git repository".into()))?;
    let root = repo
        .workdir()
        .ok_or_else(|| AppError::Other("bare repository".into()))?
        .to_string_lossy()
        .trim_end_matches(|c| c == '/' || c == '\\')
        .to_string();

    // Window focus must happen on the main/UI thread (WebView2 constraint).
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(w) = handle.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            // Targeted emit (this window only) — the explorer window must not
            // react to its own delegation request.
            let _ = w.emit("arbor://explorer-open-repo", serde_json::json!({ "repoRoot": root }));
        }
    });
    Ok(())
}
