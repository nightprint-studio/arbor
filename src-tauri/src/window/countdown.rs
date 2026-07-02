//! The **pre-recording countdown** overlay window.
//!
//! When the user starts a *video* recording with the countdown enabled, the Tyto
//! window hides itself (so the user sees their actual screen) and asks the shell to
//! open this overlay: an **opaque**, frameless, always-on-top, content-protected
//! window centered on the primary monitor, showing a big 3-2-1 animation. The
//! overlay runs its own timer (it pulls the second count on mount), and when it
//! reaches zero it reports completion and closes itself — then the store proceeds
//! to actually start the recording.
//!
//! Same design choices as [`super::region`]:
//! - **OPAQUE, never transparent** — a transparent WebView2 window receives no input
//!   on Windows and can trap the user (documented Arbor lesson). The overlay doesn't
//!   need input, but the trap is real, so it stays opaque.
//! - **PULL, not push** — the Tyto window is hidden while the countdown runs, and a
//!   freshly-shown WebView2 can miss a pushed event. So the store *polls*
//!   [`take_countdown_done`] instead of listening for an event.
//! - **content-protected** — excluded from screen capture where honored, so a
//!   racing capture never grabs the countdown digits.

use std::sync::Mutex;

use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Label for the countdown overlay window. Matches the `tyto-*` capability glob so
/// it inherits the Tyto window's permissions; `product_id_for_label` intentionally
/// does NOT map it to a product (it's transient chrome, not a product surface).
pub const TYTO_COUNTDOWN_LABEL: &str = "tyto-countdown";

const COUNTDOWN_W: f64 = 320.0;
const COUNTDOWN_H: f64 = 320.0;

/// Total seconds the overlay counts down from — pulled by the window on mount so
/// there's no emit/listen race with a just-created webview.
static COUNTDOWN_INIT: Mutex<Option<u32>> = Mutex::new(None);

/// Set by [`countdown_finished`] and taken by the store's poll. The store keeps the
/// Tyto window hidden across the countdown → recording handoff, so (like the region
/// overlay) an outgoing `invoke`/poll is reliable where a pushed event isn't.
static COUNTDOWN_DONE: Mutex<bool> = Mutex::new(false);

fn set_done(v: bool) {
    if let Ok(mut g) = COUNTDOWN_DONE.lock() {
        *g = v;
    }
}

/// Open the opaque countdown overlay, centered on the primary monitor, counting
/// down from `seconds`. The Tyto window is already hidden by the store (so the user
/// sees the real screen behind the digits) — this only builds the overlay.
#[tauri::command]
#[allow(clippy::unused_async)] // async moves the handler off the main thread (WebView2 creation).
pub async fn open_countdown_overlay(app: AppHandle, seconds: u32) {
    if let Ok(mut g) = COUNTDOWN_INIT.lock() {
        *g = Some(seconds.max(1));
    }
    set_done(false); // clear any stale outcome so the store's poll only sees THIS run
    super::dispatch_to_main(&app, "tyto-countdown", build_countdown_window);
}

/// Main-thread builder: an opaque, content-protected, always-on-top square at the
/// center of the primary monitor.
fn build_countdown_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(TYTO_COUNTDOWN_LABEL) {
        show_and_focus(&w);
        return;
    }
    let res = WebviewWindowBuilder::new(app, TYTO_COUNTDOWN_LABEL, WebviewUrl::default())
        .title("Tyto — Countdown")
        .inner_size(COUNTDOWN_W, COUNTDOWN_H)
        .decorations(false)
        .transparent(false) // MUST stay opaque — see the module doc.
        .always_on_top(true)
        .content_protected(true) // exclude the digits from a racing capture.
        .shadow(true)
        .skip_taskbar(true)
        .resizable(false)
        .focused(false) // don't steal focus from what the user is about to record.
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();
    match res {
        Ok(w) => {
            // Position after build (the codebase's proven pattern). Center of the
            // primary monitor, in logical coordinates.
            if let Ok(Some(mon)) = w.primary_monitor() {
                let scale = mon.scale_factor();
                let logical_w = mon.size().width as f64 / scale;
                let logical_h = mon.size().height as f64 / scale;
                let x = ((logical_w - COUNTDOWN_W) / 2.0).max(0.0);
                let y = ((logical_h - COUNTDOWN_H) / 2.0).max(0.0);
                let _ = w.set_position(LogicalPosition::new(x, y));
            }
        }
        Err(e) => {
            tracing::error!("failed to open tyto countdown overlay: {e}");
            // The store will time out its poll and fall back to starting immediately.
            set_done(true);
        }
    }
}

/// The overlay pulls the second count on mount (avoids an emit/listen race).
#[tauri::command]
pub fn get_countdown_init() -> Option<u32> {
    COUNTDOWN_INIT.lock().ok().and_then(|g| *g)
}

/// The overlay calls this when the digits reach zero: record completion (the store's
/// poll will see it and start the recording) and close the overlay. Deliberately does
/// NOT re-show Tyto — the recording is about to begin with Tyto still hidden.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn countdown_finished(app: AppHandle) {
    set_done(true);
    if let Ok(mut g) = COUNTDOWN_INIT.lock() {
        *g = None;
    }
    if let Some(w) = app.get_webview_window(TYTO_COUNTDOWN_LABEL) {
        let _ = w.close();
    }
}

/// The Tyto window polls this after opening the overlay. Returns `false` while the
/// countdown is still running, then `true` exactly once (it's reset on read).
#[tauri::command]
pub fn take_countdown_done() -> bool {
    match COUNTDOWN_DONE.lock() {
        Ok(mut g) => {
            let done = *g;
            if done {
                *g = false;
            }
            done
        }
        Err(_) => true, // poisoned → don't hang the store; let it proceed
    }
}

/// Abort the countdown (store error path / cancel): close the overlay and restore
/// the Tyto window. Safe to call when nothing is open.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn close_countdown_overlay(app: AppHandle) {
    if let Ok(mut g) = COUNTDOWN_INIT.lock() {
        *g = None;
    }
    set_done(true);
    if let Some(w) = app.get_webview_window(TYTO_COUNTDOWN_LABEL) {
        let _ = w.close();
    }
    if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
        show_and_focus(&w);
    }
}
