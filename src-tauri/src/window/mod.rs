//! Native window lifecycles for Arbor's standalone surfaces.
//!
//! Arbor is a single OS process that drives several **separate top-level
//! windows**, each a frameless WebView2 loading the same `index.html`; the
//! frontend root (`src/routes/+page.svelte`) branches on the window label to
//! mount the right shell. One module per window:
//!
//! - [`explorer`] — the dedicated File Explorer (`explorer` / `explorer-N`),
//!   plus its OS-global shortcut, cross-window clipboard and drag overlay.
//! - [`nemus`] — the music live-coding DAW shell (`nemus`).
//! - [`corvus`] — the Git product window (`corvus`). Today the Git UI also
//!   loads in `main`; this is the seed of the launcher split, where `main`
//!   becomes the launcher and Corvus opens as a product window.
//! - [`launcher`] — the JetBrains-Toolbox-like launcher (`launcher`).
//!   Scaffolding: backend lifecycle is ready; the frontend `LauncherShell`
//!   is still to come.
//!
//! [`events`] holds the shared `on_window_event` handler.
//!
//! Common WebView2 plumbing lives here so every window stays in sync — most
//! importantly [`WEBVIEW_BROWSER_ARGS`], which **must** match across every
//! webview in the process (and the `main` window's `additionalBrowserArgs` in
//! `tauri.conf.json`).

pub mod corvus;
pub mod events;
pub mod explorer;
pub mod launcher;
pub mod nemus;

use tauri::{AppHandle, WebviewWindow};

/// WebView2 additional browser args, shared by **every** Arbor window. Every
/// WebView2 instance in the process shares one user-data-folder + environment,
/// so creating a second webview with *different* env options fails with
/// `HRESULT 0x8007139F` (ERROR_INVALID_STATE). Must also match the `main`
/// window's `additionalBrowserArgs` in `tauri.conf.json`.
pub const WEBVIEW_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,InterestFeedContentSuggestions,WebRTC,AutofillServerCommunication";

/// Bring a window to the foreground: undo a minimize, show it, take focus. The
/// idempotent three-step every "focus the existing window" path repeats.
pub fn show_and_focus(w: &WebviewWindow) {
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
}

/// Hop to the main/UI thread before touching WebView2 windows. Window creation
/// off the main thread fails with `HRESULT 0x8007139F` ("resource not in the
/// correct state"); every `open_or_focus` entry point may run on a background
/// thread (global-shortcut handler, async command), so they all route through
/// here. `what` names the window for the error log.
pub fn dispatch_to_main(
    app: &AppHandle,
    what: &'static str,
    f: impl FnOnce(&AppHandle) + Send + 'static,
) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || f(&handle)) {
        tracing::error!("failed to dispatch {what} window to main thread: {e}");
    }
}
