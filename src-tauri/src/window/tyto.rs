//! Dedicated **Tyto** window (screen-recorder control panel) + its OS-global
//! activation shortcut.
//!
//! Tyto is Arbor's built-in screen recorder / screenshot tool (barn owl — the
//! silent watcher). Like the other products it is a frameless WebView2 window
//! loading the same `index.html`; the frontend root (`src/routes/+page.svelte`)
//! branches on the window label ([`TYTO_WINDOW_LABEL`]) to mount `TytoWindow`.
//!
//! One extra concern lives here, mirroring [`super::explorer`]: an **opt-in
//! OS-global shortcut** (default `Ctrl+Shift+R`) that opens / focuses the window
//! even when Arbor isn't focused.
//!
//! NB: the recording/encoding **engine** does not exist yet. `tyto-be` (the
//! product backend, spawned lazily by [`crate::ipc::ensure_tyto_be`]) is up and
//! serves the domain seam, but its capture handlers are stubs — so the capture UI
//! is a preview. The region selector is mocked **in-window** (an overlay inside the
//! Tyto window, not a separate OS window) so it can never trap the user; the real
//! opaque frozen-frame on-screen overlay returns with the capture engine.

use std::str::FromStr;
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::Shortcut;

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Window label for the dedicated Tyto window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into Tyto mode.
pub const TYTO_WINDOW_LABEL: &str = "tyto";

/// Full-mode window size (the standard control panel).
const TYTO_FULL_W: f64 = 1040.0;
const TYTO_FULL_H: f64 = 668.0;
const TYTO_FULL_MIN_W: f64 = 820.0;
const TYTO_FULL_MIN_H: f64 = 520.0;
/// Compact "mini" mode: a small Snip-like quick-capture toolbar.
const TYTO_MINI_W: f64 = 560.0;
const TYTO_MINI_H: f64 = 56.0;
/// Height the mini toolbar grows to while a dropdown menu is open — its popup can't
/// paint outside the WebView2 window, so the 56px strip must expand to host it.
const TYTO_MINI_MENU_H: f64 = 340.0;

/// Parse a Tauri accelerator string (e.g. `"Ctrl+Shift+R"`) into a `Shortcut`.
/// Returns `None` for an empty or unparseable string.
fn parse_accel(accel: &str) -> Option<Shortcut> {
    let a = accel.trim();
    if a.is_empty() {
        return None;
    }
    Shortcut::from_str(a).ok()
}

/// The currently-configured Tyto global shortcut, or `None` when the feature is
/// disabled or the accelerator is unparseable. Read from disk so the
/// global-shortcut press handler (which runs off the UI thread) and the
/// register/reconcile paths share one source of truth. Mirrors
/// [`super::explorer::current_explorer_shortcut`].
pub fn current_tyto_shortcut() -> Option<Shortcut> {
    let cfg = crate::config::app_config::load().ok()?;
    if !cfg.tyto.global_shortcut {
        return None;
    }
    parse_accel(&cfg.tyto.global_shortcut_accel)
}

/// Open the dedicated Tyto window, or focus it if it already exists (single
/// instance — re-summoned rather than duplicated). WebView2 window creation must
/// run on the main/UI thread — see [`super::dispatch_to_main`].
pub fn open_or_focus(app: &AppHandle) {
    // Bring up `tyto-be` while the window boots, so its shell's first `rpc` finds
    // the backend coming up. Off the main thread, idempotent — see [`ensure_backend`].
    ensure_backend(app);
    super::dispatch_to_main(app, "tyto", create_or_focus);
}

/// Bring up `tyto-be` (the screen-recorder backend) off the main thread, so the
/// spawn's blocking first-`Hello` read never stalls the UI thread. Idempotent — a
/// no-op once the backend is attached. Called from every Tyto entry point (command,
/// global shortcut) so the backend is coming up while the window boots; the capture
/// handlers are stubs today, so a missing/slow backend is harmless — the window
/// opens regardless and the FE shows its "backend in progress" state. Mirrors
/// [`super::explorer`]'s `ensure_backend`.
fn ensure_backend(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || crate::ipc::ensure_tyto_be(&app));
}

/// Main-thread body of [`open_or_focus`]. Never call directly from a command or
/// shortcut handler — go through `open_or_focus` so the thread hop happens.
fn create_or_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) {
        show_and_focus(&w);
    } else {
        build_tyto_window(app);
    }
    // Light up the launcher's Tyto node as "In esecuzione".
    super::emit_product_state(app, "tyto", true);
}

/// Build the frameless Tyto window.
///
/// `WebviewUrl::default()` resolves to the app's index (`index.html`) — the same
/// entry the main window uses — so the load path is identical in dev (Vite) and
/// packaged builds. Frameless to match Arbor; TytoShell paints its own titlebar +
/// WindowControls.
fn build_tyto_window(app: &AppHandle) {
    let res = WebviewWindowBuilder::new(app, TYTO_WINDOW_LABEL, WebviewUrl::default())
        .title("Tyto — Arbor")
        .inner_size(TYTO_FULL_W, TYTO_FULL_H)
        .min_inner_size(TYTO_FULL_MIN_W, TYTO_FULL_MIN_H)
        .decorations(false)
        .shadow(true)
        .center()
        // Build HIDDEN and reveal once TytoShell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page during
        // load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();

    match res {
        Ok(_) => super::arm_ready_reveal(app, TYTO_WINDOW_LABEL),
        Err(e) => tracing::error!("failed to open tyto window: {e}"),
    }
}

/// IPC entry point so the launcher tile and the in-app Command Palette can summon
/// the window (same window the global shortcut opens).
///
/// MUST be `async`: Tauri runs synchronous commands on the **main thread**, and
/// dispatching WebView2 window creation via `run_on_main_thread` from the main
/// thread (while it's blocked inside this command) leaves the new window with an
/// uninitialised webview. As an async command it runs off the main thread, so the
/// `run_on_main_thread` hop in `open_or_focus` behaves correctly. Mirrors
/// [`super::explorer::open_explorer_window`].
#[tauri::command]
#[allow(clippy::unused_async)] // async is load-bearing: moves the handler off the main thread.
pub async fn open_tyto_window(app: AppHandle) {
    open_or_focus(&app);
}

/// Switch the Tyto window between its **compact** (mini Snip-like toolbar, pinned
/// top-center + always-on-top) and **full** (standard control panel, centered)
/// presentations. The FE paints the matching shell; this owns the window geometry so
/// the size/placement rules live in one place. Compact drops the full-mode minimum so
/// the window can actually shrink to the toolbar size.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn set_tyto_compact(app: AppHandle, compact: bool) {
    let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) else { return };
    if compact {
        let _ = w.set_min_size(Some(LogicalSize::new(TYTO_MINI_W, TYTO_MINI_H)));
        let _ = w.set_resizable(false);
        let _ = w.set_size(LogicalSize::new(TYTO_MINI_W, TYTO_MINI_H));
        let _ = w.set_always_on_top(true);
        if let Ok(Some(mon)) = w.primary_monitor() {
            let scale = mon.scale_factor();
            let logical_w = mon.size().width as f64 / scale;
            let x = ((logical_w - TYTO_MINI_W) / 2.0).max(0.0);
            let _ = w.set_position(LogicalPosition::new(x, 12.0));
        }
    } else {
        let _ = w.set_always_on_top(false);
        let _ = w.set_min_size(Some(LogicalSize::new(TYTO_FULL_MIN_W, TYTO_FULL_MIN_H)));
        let _ = w.set_resizable(true);
        let _ = w.set_size(LogicalSize::new(TYTO_FULL_W, TYTO_FULL_H));
        let _ = w.center();
    }
    let _ = w.set_focus();
}

/// Grow the compact mini toolbar tall enough to host an in-page dropdown menu (a
/// WebView2 popup can't paint outside the window), then shrink it back on close.
/// Gated to mini mode: a no-op unless the window is currently the compact width, so
/// it can never shrink the full control panel. Keeps the top-center placement (only
/// the height changes; `set_size` leaves the top-left corner put).
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn set_tyto_mini_menu(app: AppHandle, open: bool, height: Option<f64>) {
    let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) else { return };
    // Only act in mini mode — compare the current logical width to the mini width.
    let is_mini = w
        .inner_size()
        .ok()
        .and_then(|s| w.scale_factor().ok().map(|sf| s.width as f64 / sf))
        .map(|lw| (lw - TYTO_MINI_W).abs() < 4.0)
        .unwrap_or(false);
    if !is_mini {
        return;
    }
    // The FE measures the menu's content and asks for the exact height, so the grown
    // window hugs the menu with no visible empty strip below it. Clamped so a long
    // window list can't grow past the screen.
    let h = if open {
        height.unwrap_or(TYTO_MINI_MENU_H).clamp(TYTO_MINI_H, 720.0)
    } else {
        TYTO_MINI_H
    };
    let _ = w.set_size(LogicalSize::new(TYTO_MINI_W, h));
}

// ───────────────────────────────────────────────────────────────────────────
//  OS-global shortcut registration / reconciliation
// ───────────────────────────────────────────────────────────────────────────

/// Register the configured Tyto shortcut at startup (no-op when the feature is
/// off or the accelerator is invalid). Failures are logged, not fatal. Mirrors
/// [`super::explorer::register_configured`].
#[cfg(desktop)]
pub fn register_configured(app: &AppHandle) {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if let Some(sc) = current_tyto_shortcut() {
        if let Err(e) = app.global_shortcut().register(sc) {
            tracing::warn!("failed to register tyto global shortcut: {e}");
        }
    }
}

/// Reconcile the OS-global shortcut when the Tyto config changes: unregister the
/// previously-active combo and register the new one. A combo is "active" only
/// when the feature is enabled. Returns an error (surfaced to the UI) when the new
/// accelerator is invalid or already claimed, so the settings UI can revert and
/// toast. Mirrors [`super::explorer::reconcile_global_shortcut`].
#[cfg(desktop)]
pub fn reconcile_global_shortcut(
    app: &AppHandle,
    old: &crate::config::app_config::TytoConfig,
    new: &crate::config::app_config::TytoConfig,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let old_active = old.global_shortcut.then(|| old.global_shortcut_accel.trim().to_string());
    let new_active = new.global_shortcut.then(|| new.global_shortcut_accel.trim().to_string());
    if old_active == new_active {
        return Ok(());
    }

    let gs = app.global_shortcut();
    if let Some(a) = old_active {
        if let Some(sc) = parse_accel(&a) {
            let _ = gs.unregister(sc);
        }
    }
    if let Some(a) = new_active {
        match parse_accel(&a) {
            Some(sc) => gs.register(sc).map_err(|e| format!("Couldn't register {a}: {e}"))?,
            None => return Err(format!("Invalid shortcut: {a}")),
        }
    }
    Ok(())
}
