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

use std::str::FromStr;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::Shortcut;

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

/// Parse a Tauri accelerator string (e.g. `"Ctrl+Shift+E"`) into a `Shortcut`.
/// Returns `None` for an empty or unparseable string.
fn parse_accel(accel: &str) -> Option<Shortcut> {
    let a = accel.trim();
    if a.is_empty() { return None; }
    Shortcut::from_str(a).ok()
}

/// The currently-configured explorer global shortcut, or `None` when the
/// feature is disabled or the accelerator is unparseable. Read from disk so the
/// global-shortcut press handler (which runs off the UI thread) and the
/// register/reconcile paths share one source of truth.
pub fn current_explorer_shortcut() -> Option<Shortcut> {
    let cfg = crate::config::app_config::load().ok()?;
    if !cfg.explorer.global_shortcut { return None; }
    parse_accel(&cfg.explorer.global_shortcut_accel)
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
///
/// Behaviour depends on `explorer.always_new_window`: when false (default) a
/// single explorer window is reused (re-summoning focuses it); when true a new
/// window is opened every time, each with a unique `explorer-N` label.
fn create_or_focus(app: &AppHandle) {
    let always_new = crate::config::app_config::load()
        .map(|c| c.explorer.always_new_window)
        .unwrap_or(false);

    if !always_new {
        if let Some(w) = app.get_webview_window(EXPLORER_WINDOW_LABEL) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            return;
        }
    }

    let label = next_explorer_label(app);
    // `WebviewUrl::default()` resolves to the app's index (`index.html`) — the
    // same entry the main window uses — so the load path is identical in dev
    // (Vite) and packaged builds. Frameless to match Arbor's main window; the
    // standalone shell paints its own titlebar + WindowControls.
    let res = WebviewWindowBuilder::new(app, &label, WebviewUrl::default())
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

/// Pick a free window label: the canonical `explorer` when available, otherwise
/// the first free `explorer-N`. Labels are reused once a window closes. The
/// frontend (`+page.svelte`) treats any `explorer`/`explorer-*` label as an
/// explorer window.
fn next_explorer_label(app: &AppHandle) -> String {
    if app.get_webview_window(EXPLORER_WINDOW_LABEL).is_none() {
        return EXPLORER_WINDOW_LABEL.to_string();
    }
    for i in 2..1000 {
        let label = format!("{EXPLORER_WINDOW_LABEL}-{i}");
        if app.get_webview_window(&label).is_none() {
            return label;
        }
    }
    // Absurd fallback (1000 explorer windows open) — reuse the canonical label.
    EXPLORER_WINDOW_LABEL.to_string()
}

/// IPC entry point so the in-app Command Palette ("Open File Explorer in New
/// Window") can summon the same window the global shortcut does.
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**,
/// and dispatching WebView2 window creation via `run_on_main_thread` from the
/// main thread (while it's blocked inside this command) leaves the new window
/// with an uninitialised webview — a blank window with no devtools. As an async
/// command it runs on the async runtime (a background thread), so the
/// `run_on_main_thread` hop in `open_or_focus` behaves exactly like the
/// global-shortcut handler (which also runs off the main thread).
#[tauri::command]
#[allow(clippy::unused_async)] // async is load-bearing here: it moves the
// handler off the main thread (see doc comment) — there's nothing to await.
pub async fn open_explorer_window(app: AppHandle) {
    open_or_focus(&app);
}

/// Register the configured explorer shortcut at startup (no-op when the feature
/// is off or the accelerator is invalid). Failures are logged, not fatal.
#[cfg(desktop)]
pub fn register_configured(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if let Some(sc) = current_explorer_shortcut() {
        if let Err(e) = app.global_shortcut().register(sc) {
            tracing::warn!("failed to register explorer global shortcut: {e}");
        }
    }
}

/// Reconcile the OS-global shortcut when the explorer config changes: unregister
/// the previously-active combo and register the new one. A combo is "active"
/// only when the feature is enabled. Returns an error (surfaced to the UI) when
/// the new accelerator is invalid or already claimed by another app, so the
/// settings UI can revert and toast.
#[cfg(desktop)]
pub fn reconcile_global_shortcut(
    app: &AppHandle,
    old: &crate::config::app_config::ExplorerConfig,
    new: &crate::config::app_config::ExplorerConfig,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    // Compare by (enabled, accel) so identical settings short-circuit.
    let old_active = old.global_shortcut.then(|| old.global_shortcut_accel.trim().to_string());
    let new_active = new.global_shortcut.then(|| new.global_shortcut_accel.trim().to_string());
    if old_active == new_active { return Ok(()); }

    let gs = app.global_shortcut();
    if let Some(a) = old_active {
        if let Some(sc) = parse_accel(&a) { let _ = gs.unregister(sc); }
    }
    if let Some(a) = new_active {
        match parse_accel(&a) {
            Some(sc) => gs.register(sc).map_err(|e| format!("Couldn't register {a}: {e}"))?,
            None => return Err(format!("Invalid shortcut: {a}")),
        }
    }
    Ok(())
}
