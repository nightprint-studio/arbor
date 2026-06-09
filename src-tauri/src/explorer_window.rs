//! Dedicated File Explorer window + its OS-global activation shortcut.
//!
//! `Ctrl+Shift+E` (registered system-wide via `tauri-plugin-global-shortcut`,
//! so it fires even when Arbor isn't focused) opens a standalone, Arbor-styled
//! window that hosts ONLY the built-in file explorer — not the full app.
//!
//! The window loads the same `index.html` as the main window; the frontend
//! root (`src/routes/+page.svelte`) branches on the window label
//! ([`EXPLORER_WINDOW_LABEL`]) to mount the standalone explorer shell
//! (`ExplorerWindow.svelte`) instead of `AppShell`. This avoids a second
//! SvelteKit route / prerender entirely — both windows share one entry point.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Window label for the dedicated explorer window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into explorer mode.
pub const EXPLORER_WINDOW_LABEL: &str = "explorer";

/// WebView2 additional browser args. **Must match the `main` window's
/// `additionalBrowserArgs` in `tauri.conf.json`** — every WebView2 instance in
/// the process shares one user-data-folder + environment, and creating a second
/// webview with *different* env options fails with `HRESULT 0x8007139F`
/// (ERROR_INVALID_STATE). Keep these two in sync.
const WEBVIEW_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,Translate,InterestFeedContentSuggestions,WebRTC,AutofillServerCommunication";

/// The OS-global hotkey that opens the explorer window: `Ctrl+Shift+E`.
///
/// Defined as a function (not a const) so both the plugin's press handler in
/// `lib.rs` and the `setup()` registration build the exact same `Shortcut`
/// without sharing state.
pub fn explorer_shortcut() -> Shortcut {
    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyE)
}

/// Open the dedicated explorer window, or focus it if it already exists
/// (single-instance: one explorer window, re-summoned rather than duplicated).
///
/// Both entry points (the global-shortcut handler and the `open_explorer_window`
/// IPC command) run on **background** threads, but WebView2 window creation must
/// happen on the **main/UI** thread — otherwise it fails with
/// `HRESULT 0x8007139F` ("resource not in the correct state"). So we always hop
/// to the main thread before touching windows.
pub fn open_or_focus(app: &AppHandle) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || create_or_focus(&handle)) {
        tracing::error!("failed to dispatch explorer window to main thread: {e}");
    }
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(EXPLORER_WINDOW_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }

    // `WebviewUrl::default()` resolves to the app's index (`index.html`) — the
    // same entry the main window uses — so the load path is identical in dev
    // (Vite) and packaged builds. Frameless to match Arbor's main window; the
    // standalone shell paints its own titlebar + WindowControls.
    let res = WebviewWindowBuilder::new(app, EXPLORER_WINDOW_LABEL, WebviewUrl::default())
        .title("File Explorer — Arbor")
        .inner_size(1100.0, 720.0)
        .min_inner_size(720.0, 460.0)
        .decorations(false)
        .shadow(true)
        .center()
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();

    if let Err(e) = res {
        tracing::error!("failed to open explorer window: {e}");
    }
}

/// IPC entry point so the in-app Command Palette ("Open File Explorer in New
/// Window") can summon the same window the global shortcut does.
#[tauri::command]
pub fn open_explorer_window(app: AppHandle) {
    open_or_focus(&app);
}
