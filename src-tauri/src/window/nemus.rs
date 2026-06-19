//! Dedicated **nemus** window — the music live-coding DAW shell.
//!
//! `nemus` is Arbor's built-in music live-coding tool (a small language + audio
//! engine; see `design/nemus/`). Its authoring surface is a standalone,
//! Arbor-styled window — NOT the full Git app and NOT a plugin. This module is
//! the window's lifecycle: a frameless window that loads the same `index.html`,
//! with the frontend root (`src/routes/+page.svelte`) branching on the window
//! label ([`NEMUS_WINDOW_LABEL`]) to mount `NemusWindow.svelte` instead of
//! `AppShell`.
//!
//! Deliberately minimal compared to [`super::explorer`]: a single reusable
//! window, no global shortcut, no cross-window clipboard/drag plumbing (all of
//! which the explorer needs and nemus does not).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated nemus window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into nemus mode.
pub const NEMUS_WINDOW_LABEL: &str = "nemus";

/// Open the dedicated nemus window, or focus it if it already exists (single
/// window, re-summoned rather than duplicated). WebView2 window creation must
/// run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "nemus", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(NEMUS_WINDOW_LABEL) {
        show_and_focus(&w);
        return;
    }
    build_nemus_window(app);
}

/// Build the frameless nemus window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the main window uses — so the load path is identical in dev (Vite) and
/// packaged builds. Frameless to match Arbor; the NemusShell paints its own
/// titlebar + window controls. Sized larger than the explorer: a DAW needs room
/// for the tracks viz, the editor and the bottom console side by side.
fn build_nemus_window(app: &AppHandle) {
    let res = WebviewWindowBuilder::new(app, NEMUS_WINDOW_LABEL, WebviewUrl::default())
        .title("nemus — Arbor")
        // `inner_size` is the *restore* size (what you get after un-maximising);
        // the window opens maximised so a DAW lands full-screen, not at the small
        // explorer footprint.
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        .maximized(true)
        .decorations(false)
        .shadow(true)
        .center()
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();

    if let Err(e) = res {
        tracing::error!("failed to open nemus window: {e}");
    }
}

/// IPC entry point so the in-app Command Palette ("Open nemus (Music)") can
/// summon the window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command
/// it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly.
#[tauri::command]
#[allow(clippy::unused_async)] // async is load-bearing: it moves the handler off
// the main thread (see doc comment) — there's nothing to await.
pub async fn open_nemus_window(app: AppHandle) {
    open_or_focus(&app);
}
