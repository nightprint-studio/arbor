//! Git awareness for the built-in File Explorer — shell-side glue.
//!
//! The status overlays, inline light actions (stage / unstage / discard /
//! ignore), branch list/switch, and remote-url lookup all moved to the corvus
//! broker, now served out-of-process by `corvus-be`
//! (`crates/products/corvus/be/src/fs_git.rs`), which carries the shared per-repo status
//! cache with them. The only command left here is the heavy-action delegation,
//! which is genuine shell glue: it needs an `AppHandle` to bring the main window
//! forward (a WebView2 main-thread operation) and emit a targeted event to it.
//!
//! The repo-root is resolved by sitta-be (libgit2 `explorer::repo_root`) before
//! this command runs, so the launcher executes no git at all — it only does the
//! window-glue (focus the main window + emit the open-repo event).

use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;

/// Bring the main Arbor window forward and ask it to open the repo at `repo_root`.
/// The caller (the File Explorer) resolves the workdir via sitta-be first, so this
/// is pure window-glue — no git here. The heavy git operations (diff / log / blame
/// / commit) live in the main window's full git UI; the explorer just delegates.
#[tauri::command]
pub fn fs_open_in_arbor(app: AppHandle, repo_root: String) -> Result<(), AppError> {
    if repo_root.trim().is_empty() {
        return Err(AppError::Other("not inside a git repository".into()));
    }
    let root = repo_root.trim_end_matches(['/', '\\']).to_string();

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
