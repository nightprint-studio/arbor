//! Git awareness for the built-in File Explorer — shell-side glue.
//!
//! The status overlays, inline light actions (stage / unstage / discard /
//! ignore), branch list/switch, and remote-url lookup all moved to the corvus
//! broker, now served out-of-process by `corvus-be`
//! (`crates/products/corvus/be/src/fs_git.rs`), which carries the shared per-repo status
//! cache with them. The only command left here is the heavy-action delegation,
//! which is genuine shell glue: it needs an `AppHandle` to bring the Corvus window
//! forward (a WebView2 main-thread operation) and emit a targeted event to it.
//!
//! The repo-root is resolved by sitta-be (libgit2 `explorer::repo_root`) before
//! this command runs, so the launcher executes no git at all — it only does the
//! window-glue (focus the main window + emit the open-repo event).

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::error::AppError;
use crate::window::corvus::{open_corvus_window, CORVUS_WINDOW_LABEL};

/// How long a Corvus window that is being built is waited for before the request is sent anyway.
const WINDOW_WAIT: Duration = Duration::from_secs(20);
/// After a freshly built window is revealed: its `AppShell` registers the listener a moment around the first paint.
const LISTENER_GRACE: Duration = Duration::from_millis(400);

/// Open (or focus) the **Corvus** window and ask it to open the repo at `repo_root`.
/// The caller (the File Explorer, Bennu) resolves the workdir first, so this is pure
/// window-glue — no git here. The heavy git operations (diff / log / blame / commit)
/// live in Corvus's full git UI; the caller just delegates.
///
/// Corvus has a window of its own — `main` is the launcher, which neither mounts the Git
/// shell nor listens for the request, so sending it there only raised the program picker.
/// When the window has to be built the request waits for it to be revealed, because an
/// event emitted before its listener exists is an event nobody receives.
///
/// `async` for the same reason as [`open_corvus_window`]: window creation hops to the main
/// thread and must not be requested from it.
#[tauri::command]
pub async fn fs_open_in_arbor(app: AppHandle, repo_root: String) -> Result<(), AppError> {
    if repo_root.trim().is_empty() {
        return Err(AppError::Other("not inside a git repository".into()));
    }
    let root = repo_root.trim_end_matches(['/', '\\']).to_string();

    let already_open = app.get_webview_window(CORVUS_WINDOW_LABEL).is_some();
    open_corvus_window(app.clone()).await;
    let window = revealed_corvus_window(&app).await.ok_or_else(|| AppError::Other("the Corvus window could not be opened".into()))?;
    if !already_open {
        tokio::time::sleep(LISTENER_GRACE).await;
    }
    // Targeted emit (this window only) — the caller's own window must not react to its own delegation.
    window
        .emit("arbor://explorer-open-repo", serde_json::json!({ "repoRoot": root }))
        .map_err(|e| AppError::Other(e.to_string()))
}

/// The Corvus window once it exists and has been shown — or as it is when [`WINDOW_WAIT`] runs out
/// (a window mode that keeps it hidden never reports visible).
async fn revealed_corvus_window(app: &AppHandle) -> Option<WebviewWindow> {
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < WINDOW_WAIT {
        if let Some(w) = app.get_webview_window(CORVUS_WINDOW_LABEL) {
            if w.is_visible().unwrap_or(false) {
                return Some(w);
            }
        }
        tokio::time::sleep(step).await;
        waited += step;
    }
    app.get_webview_window(CORVUS_WINDOW_LABEL)
}
