//! Dedicated **launcher** window — the JetBrains-Toolbox-like product launcher.
//!
//! The launcher is where Arbor is headed: `src-tauri` becomes a broker + UI
//! container + launcher, and each product (Corvus/Git, merula/Music, …) opens in
//! its own window ([`super::corvus`], [`super::merula`]). The launcher is the
//! home screen — recent repos/projects, product tiles, settings — from which
//! those windows are summoned.
//!
//! **Scaffolding.** The window lifecycle is ready (frameless, single reusable
//! window, main-thread-safe open), so the launcher can be opened/focused and
//! the frontend can branch on [`LAUNCHER_WINDOW_LABEL`]. The actual
//! `LauncherShell.svelte` does not exist yet — until it does, this window
//! renders the default `index.html` route. Wire the frontend branch in
//! `src/routes/+page.svelte` (alongside the `explorer` / `merula` cases) when the
//! shell lands.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated launcher window. The frontend reads
/// `getCurrentWindow().label` and matches this to mount `LauncherShell`.
pub const LAUNCHER_WINDOW_LABEL: &str = "launcher";

/// Open the launcher window, or focus it if it already exists. WebView2 window
/// creation must run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    super::dispatch_to_main(app, "launcher", create_or_focus);
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(LAUNCHER_WINDOW_LABEL) {
        show_and_focus(&w);
        return;
    }
    build_launcher_window(app);
}

/// Build the frameless launcher window. Compact, fixed-ish home-screen footprint
/// (a toolbox, not a workspace), centred and NOT maximised — unlike the product
/// windows. The shell paints its own titlebar + window controls.
fn build_launcher_window(app: &AppHandle) {
    let builder = WebviewWindowBuilder::new(app, LAUNCHER_WINDOW_LABEL, WebviewUrl::default())
        .title("Arbor")
        .inner_size(960.0, 640.0)
        .min_inner_size(720.0, 480.0)
        .shadow(true)
        .center()
        // Build HIDDEN and reveal once the launcher shell has painted (window_ready) —
        // an opaque WebView2 window would otherwise flash its white default page during
        // load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    // Native traffic lights on macOS, frameless elsewhere (see super::native_titlebar).
    let res = super::native_titlebar(builder).build();

    match res {
        Ok(_) => super::arm_ready_reveal(app, LAUNCHER_WINDOW_LABEL),
        Err(e) => tracing::error!("failed to open launcher window: {e}"),
    }
}

/// IPC entry point so the Command Palette / a product window's "Back to
/// launcher" action can summon the launcher.
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
pub async fn open_launcher_window(app: AppHandle) {
    open_or_focus(&app);
}
