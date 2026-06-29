//! Git awareness for the built-in File Explorer — shell-side glue.
//!
//! The status overlays, inline light actions (stage / unstage / discard /
//! ignore), branch list/switch, and remote-url lookup all moved to the corvus
//! broker, now served out-of-process by `corvus-be`
//! (`crates/corvus/be/src/fs_git.rs`), which carries the shared per-repo status
//! cache with them. The only command left here is the heavy-action delegation,
//! which is genuine shell glue: it needs an `AppHandle` to bring the main window
//! forward (a WebView2 main-thread operation) and emit a targeted event to it.
//!
//! The repo-root lookup goes through the git CLI (`git -C <path> rev-parse
//! --show-toplevel`) rather than a `git2` handle, so the launcher needs no
//! libgit2 just to find the workdir a filesystem path belongs to.

use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;

/// Bring the main Arbor window forward and ask it to open the repo containing
/// `path`. The heavy git operations (diff / log / blame / commit) live in the
/// main window's full git UI; the explorer just delegates to it.
#[tauri::command]
pub fn fs_open_in_arbor(app: AppHandle, path: String) -> Result<(), AppError> {
    use crate::process_ext::NoWindowExt;

    let not_a_repo = || AppError::Other("not inside a git repository".into());
    let out = crate::git_cli::command()
        .args(["-C"])
        .arg(&path)
        .args(["rev-parse", "--show-toplevel"])
        .no_window()
        .output()
        .map_err(|_| not_a_repo())?;
    if !out.status.success() {
        return Err(not_a_repo());
    }
    let root = String::from_utf8(out.stdout)
        .map_err(|_| AppError::Other("git output is not valid UTF-8".into()))?
        .trim()
        .trim_end_matches(['/', '\\'])
        .to_string();
    if root.is_empty() {
        return Err(not_a_repo());
    }

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
