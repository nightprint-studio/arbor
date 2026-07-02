//! Dedicated **merula** window — the music live-coding DAW shell.
//!
//! `merula` is Arbor's built-in music live-coding tool (a small language + audio
//! engine; see `design/merula/`). Its authoring surface is a standalone,
//! Arbor-styled window — NOT the full Git app and NOT a plugin. This module is
//! the window's lifecycle: a frameless window that loads the same `index.html`,
//! with the frontend root (`src/routes/+page.svelte`) branching on the window
//! label ([`MERULA_WINDOW_LABEL`]) to mount `MerulaWindow.svelte` instead of
//! `AppShell`.
//!
//! Deliberately minimal compared to [`super::explorer`]: a single reusable
//! window, no global shortcut, no cross-window clipboard/drag plumbing (all of
//! which the explorer needs and merula does not).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated merula window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into merula mode.
pub const MERULA_WINDOW_LABEL: &str = "merula";

/// Open the dedicated merula window, or focus it if it already exists (single
/// window, re-summoned rather than duplicated). WebView2 window creation must
/// run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "merula", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MERULA_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_merula_window(app);
    }
    // Light up the launcher's Merula node as "In esecuzione".
    super::emit_product_state(app, "merula", true);
}

/// Build the frameless merula window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the main window uses — so the load path is identical in dev (Vite) and
/// packaged builds. Frameless to match Arbor; the MerulaShell paints its own
/// titlebar + window controls. Sized larger than the explorer: a DAW needs room
/// for the tracks viz, the editor and the bottom console side by side.
fn build_merula_window(app: &AppHandle) {
    let res = WebviewWindowBuilder::new(app, MERULA_WINDOW_LABEL, WebviewUrl::default())
        .title("merula — Arbor")
        // `inner_size` is the *restore* size (what you get after un-maximising);
        // the window opens maximised so a DAW lands full-screen, not at the small
        // explorer footprint.
        .inner_size(1320.0, 860.0)
        .min_inner_size(900.0, 600.0)
        .maximized(true)
        .decorations(false)
        .shadow(true)
        .center()
        // Build HIDDEN and reveal once the shell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page during
        // load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();

    match res {
        Ok(_) => super::arm_ready_reveal(app, MERULA_WINDOW_LABEL),
        Err(e) => tracing::error!("failed to open merula window: {e}"),
    }
}

/// IPC entry point so the in-app Command Palette ("Open merula (Music)") can
/// summon the window.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview — a blank window with no devtools. As an async command
/// it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly.
#[tauri::command]
pub async fn open_merula_window(app: AppHandle) {
    // Bring up the audio backend before the window's shell loads and fires its
    // first BE-required `rpc`. Run `ensure_merula_be` on the BLOCKING POOL, never
    // on a runtime worker: it parks on synchronous framed-IPC (`rx.recv()`) and
    // can trigger reverse-channel host round-trips that need free runtime workers
    // (`block_on`). Blocking a worker here starves that path → blank-window
    // deadlock that also freezes the launcher. `spawn_blocking` keeps the workers
    // free while we await the backend coming up. Idempotent — a no-op when Merula
    // is re-summoned and the backend is already up. Same shape as
    // `corvus::open_corvus_window`.
    let app_be = app.clone();
    let _ = tokio::task::spawn_blocking(move || crate::ipc::ensure_merula_be(&app_be)).await;
    open_or_focus(&app);
}
