//! IPC surface for the `arbor://…` deep-link router — host-coupled subset.
//!
//! Only the handlers that need the Tauri shell live here:
//!
//!   * **Cold-start delivery** — `deep_link_ready` lets the frontend tell
//!     the backend that its `arbor://deep-link` listener is mounted, after
//!     which the URL buffer is flushed and future links emit immediately.
//!
//!   * **Manual dispatch** — `dispatch_deep_link` brings the main window
//!     forward and emits the trusted `arbor://deep-link-manual` channel.
//!
//! The leaf handlers (`find_repo_by_remote_url`, get/set `[deep_link]` config)
//! moved to the platform backend — see `ipc/platform/deep_link.rs`.

use tauri::{AppHandle, Emitter, Manager};

use crate::AppState;
use crate::error::Result;

// ---------------------------------------------------------------------------
// Cold-start delivery
// ---------------------------------------------------------------------------

/// Called once by the frontend (`AppShell.onMount`) after it has registered
/// its `arbor://deep-link` event listener.  Drains any URLs that arrived
/// during cold-start and switches the buffer to direct-emit mode.
#[tauri::command]
pub fn deep_link_ready(app: AppHandle) -> Result<()> {
    let state = app.state::<AppState>();
    state.deep_link_buffer.mark_ready_and_flush(&app);
    Ok(())
}

/// Dispatch an `arbor://…` deep link the user typed MANUALLY into the File
/// Explorer address bar (including the standalone explorer window, which has no
/// dispatcher of its own). Brings the main window forward and emits the
/// `arbor://deep-link-manual` event — a *trusted* channel distinct from the
/// `arbor://deep-link` one used for externally-delivered (OS) links. The
/// frontend dispatcher treats trusted links as explicit user intent and skips
/// the enable gates (the per-action confirm prompt still applies). The URL is
/// not validated here — an unrecognised one surfaces the dispatcher's toast.
#[tauri::command]
pub fn dispatch_deep_link(app: AppHandle, url: String) -> Result<()> {
    // Route to the Corvus window (the Git product), not the launcher ("main").
    // Open/focus it and ensure corvus-be is up off the command thread, then emit
    // the trusted channel to its AppShell listener. (When the link was typed in
    // an already-open Corvus/explorer the ensure is a no-op and the focus + emit
    // land immediately.)
    let h = app.clone();
    tauri::async_runtime::spawn(async move {
        // `ensure_corvus_be` parks on synchronous framed-IPC and can trigger the
        // reverse-channel credential round-trip (which needs free runtime workers
        // for its `block_on`), so it must run on the BLOCKING POOL — never inline
        // on this runtime worker, or it starves that path into the blank-window
        // deadlock (see `window::corvus::open_corvus_window`).
        let h_be = h.clone();
        let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_corvus_be(&h_be)).await;
        crate::window::corvus::open_or_focus(&h);
    });
    let _ = app.emit("arbor://deep-link-manual", url);
    Ok(())
}
