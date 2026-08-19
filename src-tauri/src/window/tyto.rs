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
//! `tyto-be` (the product backend, spawned lazily by
//! [`crate::ipc::ensure_tyto_be`]) drives the real capture engine: screen/window
//! recording (scap), system-audio loopback + microphone, ffmpeg muxing, and GDI
//! screenshots. The shell here owns only the OS-integration glue (window, global
//! shortcut, recording HUD, in-window selector geometry).
//!
//! Tyto has two in-window presentations of the same WebView2 window (no separate
//! overlay window — an opaque WebView2 can never trap the user): the **Snip
//! selector** (the window grown to cover one monitor over a frozen backdrop, driven
//! by [`set_tyto_selection`]) and the **full control panel**. The OS-global shortcut
//! drops straight into the Snip selector via the [`SNIP_INTENT`] flag; from there the
//! user can expand to the full panel.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::Shortcut;

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Set when Tyto is summoned via its OS-global shortcut (the quick-capture entry
/// point): the window should drop straight into the in-window Snip selector rather than
/// the full control panel. Consumed by the FE via [`take_tyto_snip_intent`] on mount
/// (fresh window) or on the `tyto://enter-snip` event (already-open window).
static SNIP_INTENT: AtomicBool = AtomicBool::new(false);

/// Event pushed to an already-open Tyto window so it enters the Snip selector (a fresh
/// window can miss it during mount, so it uses the pull-flag [`take_tyto_snip_intent`]).
const TYTO_ENTER_SNIP_EVENT: &str = "tyto://enter-snip";

/// Pushed to the Tyto window once the OS has answered the screen-recording
/// permission (payload: granted). The window is usually already up by then — the
/// dialog blocks for as long as the user reads it — so the frontend re-enumerates
/// its sources instead of keeping the "no permission" state it fetched a moment too
/// early.
const TYTO_CAPTURE_PERMISSION_EVENT: &str = "tyto://capture-permission";

/// Event pushed to the recording HUD to stop the active recording — fired when the
/// OS-global Tyto shortcut is pressed *while a recording is running* (so the same key
/// that starts a capture also stops it, from anywhere, without surfacing Tyto). The HUD
/// listens and runs its normal stop (finalize + save). See [`request_stop_recording`].
const TYTO_GLOBAL_STOP_EVENT: &str = "tyto://global-stop";

/// True while a video recording is in progress (set when the HUD opens, cleared when it
/// closes). Read by the global-shortcut handler so a press *during* a recording stops it
/// instead of opening a new selector. `Relaxed` is fine: single flag, no ordering deps.
static TYTO_RECORDING: AtomicBool = AtomicBool::new(false);

/// Mark the recording state (called by the HUD open/close so the global shortcut knows
/// whether a press should stop vs. start a capture).
pub fn set_recording(active: bool) {
    TYTO_RECORDING.store(active, Ordering::Relaxed);
}

/// True while a recording is running (the HUD is up).
pub fn is_recording() -> bool {
    TYTO_RECORDING.load(Ordering::Relaxed)
}

/// Ask the running recording to stop, from anywhere: push the stop event to the HUD
/// window (which owns the stop flow — finalize the file, tear itself down, restore Tyto).
/// No-op if the HUD isn't around. Fired by the global-shortcut handler when pressed
/// mid-recording.
pub fn request_stop_recording(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(super::hud::TYTO_HUD_LABEL) {
        let _ = w.emit(TYTO_GLOBAL_STOP_EVENT, ());
    }
}

/// Window label for the dedicated Tyto window. The frontend reads
/// `getCurrentWindow().label` and matches this to switch into Tyto mode.
pub const TYTO_WINDOW_LABEL: &str = "tyto";

/// Full-mode window size (the standard control panel). The compact presentation is now
/// the in-window fullscreen Snip selector (see [`set_tyto_selection`]), not a mini bar.
const TYTO_FULL_W: f64 = 1040.0;
const TYTO_FULL_H: f64 = 668.0;
const TYTO_FULL_MIN_W: f64 = 820.0;
const TYTO_FULL_MIN_H: f64 = 520.0;

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
/// global shortcut) so the backend is coming up while the window boots. A missing/slow
/// backend is harmless — the window opens regardless and the FE degrades to its mock
/// state until the engine attaches. Mirrors [`super::explorer`]'s `ensure_backend`.
///
/// A sibling thread asks for the **screen-recording permission**, and this is the
/// right moment for both reasons: opening the recorder is when the user has said what
/// they want the permission for, and the ask comes from the app bundle rather than
/// from a headless child whose TCC identity is inherited rather than owned. It blocks
/// while the system dialog is up, which is why it lives on a detached thread and not
/// on the UI thread or a runtime worker. The window opens regardless — a refusal is
/// reported by the source picker, not by a missing window.
///
/// `tyto-be` keeps its own ask (see its `capture::access`) rather than trusting this
/// one: it can be spawned without a window at all, by an AI client calling the record
/// tool. macOS shows one dialog however many times it is asked, so the overlap costs
/// nothing and the backend stays able to stand on its own.
fn ensure_backend(app: &AppHandle) {
    // TWO threads, not two statements: the permission dialog blocks for as long as the
    // user takes to read it, and the backend spawn must not queue behind that — a
    // recorder whose engine only starts once a dialog is dismissed is a recorder that
    // looks broken for the whole time the dialog is up.
    {
        let app = app.clone();
        std::thread::spawn(move || {
            let granted = super::screen_capture::request_if_needed();
            if !granted {
                tracing::info!("tyto: screen-recording permission not granted — the picker will say so");
            }
            // The window exists by now in the case that matters (the dialog took time);
            // when the permission was already settled this returns instantly and the
            // emit is a harmless no-op against a window that may not be up yet.
            if let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) {
                let _ = w.emit(TYTO_CAPTURE_PERMISSION_EVENT, granted);
            }
        });
    }

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
    let builder = WebviewWindowBuilder::new(app, TYTO_WINDOW_LABEL, WebviewUrl::default())
        .title("Tyto — Arbor")
        .inner_size(TYTO_FULL_W, TYTO_FULL_H)
        .min_inner_size(TYTO_FULL_MIN_W, TYTO_FULL_MIN_H)
        .shadow(true)
        .center()
        // Build HIDDEN and reveal once TytoShell has painted (window_ready) — an
        // opaque WebView2 window would otherwise flash its white default page during
        // load. See super::window_ready / arm_ready_reveal.
        .visible(false)
        // Match the main window's WebView2 env (see WEBVIEW_BROWSER_ARGS) —
        // mismatched args on a second webview → HRESULT 0x8007139F.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS);
    // Native traffic lights on macOS, frameless elsewhere (see super::native_titlebar).
    let res = super::native_titlebar(builder).build();

    match res {
        Ok(w) => {
            super::apply_product_icon(&w, "tyto");
            super::arm_ready_reveal(app, TYTO_WINDOW_LABEL)
        }
        Err(e) => tracing::error!("failed to open tyto window: {e}"),
    }
}

/// Open/focus Tyto AND request the in-window Snip selector — the OS-global-shortcut
/// entry point (quick capture, like Win+Shift+S). Sets a pull-flag the FE consumes on
/// mount (fresh window) and pushes an event for an already-open window; either way the
/// FE drops into the selector once the backend is ready. The launcher tile /
/// Command-Palette path ([`open_tyto_window`]) does NOT call this — it opens the full
/// control panel.
pub fn open_or_focus_snip(app: &AppHandle) {
    SNIP_INTENT.store(true, Ordering::Relaxed);
    open_or_focus(app);
    // Fresh windows aren't built yet here (creation is dispatched to the main thread),
    // so this emit only reaches an ALREADY-open window; the fresh case uses the pull-flag.
    if let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) {
        let _ = w.emit(TYTO_ENTER_SNIP_EVENT, ());
    }
}

/// The FE pulls this on mount (and on the `tyto://enter-snip` event) to learn whether
/// Tyto was summoned via the global shortcut and should enter the Snip selector. Take +
/// clear, so a later normal open (launcher tile) doesn't inherit a stale intent.
#[tauri::command]
pub fn take_tyto_snip_intent() -> bool {
    SNIP_INTENT.swap(false, Ordering::Relaxed)
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


/// Drive the Tyto window in/out of its **in-window fullscreen selector** (the
/// Windows-Snip-style capture picker). Unlike compact/mini this doesn't shrink the
/// window — it grows it to *cover one monitor* so the frozen backdrop + toolbar are
/// painted edge-to-edge on that display, then restores the full control panel on exit.
/// The FE freezes the target monitor and passes its **physical** bounds; the shell owns
/// the geometry so the cover/restore rules live in one place.
///
/// `x`/`y`/`width`/`height` are **PHYSICAL** pixels. Using physical (not logical) is what
/// makes the monitor-SWITCH robust: `set_position(LogicalPosition)` is interpreted against
/// the window's *current* monitor scale, so moving a window onto a display with a
/// different DPI mis-places/mis-sizes it (the "zoom/resolution goes wrong on switch" bug).
/// Physical coordinates are absolute across monitors; the webview then re-derives its own
/// device-pixel-ratio on the new display so the frozen backdrop still fills exactly.
///
/// * `active = true` (enter): drop the minimum size, lock resizing, size+place the
///   window to the monitor's physical bounds, pin always-on-top, focus.
/// * `active = false` (exit): un-pin, restore the full-mode minimum + resizability,
///   size back to the standard panel, re-center.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn set_tyto_selection(app: AppHandle, active: bool, x: i32, y: i32, width: u32, height: u32) {
    let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) else { return };
    if active {
        // Drop the full-mode minimum first so the cover size (which may be smaller than
        // TYTO_FULL_MIN_* on a low-res monitor) can actually be applied.
        let _ = w.set_min_size(Some(PhysicalSize::new(1u32, 1u32)));
        let _ = w.set_resizable(false);
        // PHYSICAL bounds — absolute across monitors, so a switch to a different-DPI
        // display lands and sizes correctly (see the doc note).
        let _ = w.set_size(PhysicalSize::new(width, height));
        let _ = w.set_position(PhysicalPosition::new(x, y));
        let _ = w.set_always_on_top(true);
        let _ = w.set_focus();
    } else {
        reset_to_full_panel(&w);
        let _ = w.set_focus();
    }
}

/// Restore the Tyto window to its standard control-panel geometry: un-pin always-on-top,
/// re-apply the full-mode minimum + resizability, size back to the panel, re-center.
/// Deliberately does NOT show/focus the window, so it's safe to call while it's HIDDEN
/// (e.g. resetting the covering "selector" bounds during a recording, before the window
/// is shown again by the HUD teardown). `set_focus` would reveal a hidden window, so the
/// visible-restore paths (`set_tyto_selection(false)`) add it themselves.
pub(crate) fn reset_to_full_panel(w: &WebviewWindow) {
    let _ = w.set_always_on_top(false);
    let _ = w.set_min_size(Some(LogicalSize::new(TYTO_FULL_MIN_W, TYTO_FULL_MIN_H)));
    let _ = w.set_resizable(true);
    let _ = w.set_size(LogicalSize::new(TYTO_FULL_W, TYTO_FULL_H));
    let _ = w.center();
}

/// Reset the Tyto window to the full control-panel geometry WITHOUT showing/focusing it.
/// The FE calls this on the recording-start error path (the window may still be at its
/// monitor-covering "countdown" bounds) so a subsequent `show()` reveals the normal panel,
/// not a monitor-sized blank. The normal teardown resets bounds in `close_recording_hud`.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn reset_tyto_bounds(app: AppHandle) {
    if let Some(w) = app.get_webview_window(TYTO_WINDOW_LABEL) {
        reset_to_full_panel(&w);
    }
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
