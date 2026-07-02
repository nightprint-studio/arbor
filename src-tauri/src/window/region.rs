//! The **frozen-frame region selector** window.
//!
//! When the user picks a capture region, `tyto-be` grabs a frozen screenshot of
//! the target monitor to a temp PNG (`freeze_screen`) and the Tyto window asks the
//! shell to open this overlay: an **opaque** (`transparent(false)`), frameless,
//! always-on-top window sized to that monitor, showing the frozen PNG full-bleed.
//! The user drags a rectangle; confirm routes the CSS-pixel rect back to the Tyto
//! window (which resolves it to physical pixels via `select_region`), cancel just
//! restores Tyto.
//!
//! OPAQUE, never transparent: a transparent WebView2 window on Windows receives no
//! input and would trap the user (documented Arbor lesson). The frozen screenshot
//! is what lets an opaque window still look like a live-screen overlay.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Event pushed to the (reused) overlay so it reloads its init + frozen frame for the
/// next selection. Only fired when the window already exists — a fresh build pulls its
/// init on mount instead.
const REGION_REINIT_EVENT: &str = "tyto://region-reinit";

/// Label for the region-selection window. Matches the `tyto-*` capability glob so
/// it inherits the Tyto window's permissions; `product_id_for_label` intentionally
/// does NOT map it to a product (it's transient chrome, not a product surface).
pub const TYTO_REGION_LABEL: &str = "tyto-region";

/// A UI-element rect in monitor-local CSS pixels (for the overlay's smart pick).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ElemRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// One hover-target window in the picker overlay, in **virtual-desktop CSS pixels**.
/// `id` = `win-<hwnd>` (matches the source picker + `tyto-be`'s `enumerate_pick_targets`).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct WinRect {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// One hover-target monitor in the picker overlay, in **virtual-desktop CSS pixels**.
/// `id` = `mon-<hmonitor>` (matches the source picker + `tyto-be`).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MonRect {
    pub id: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Init payload the region window pulls on mount (avoids an emit/listen race).
#[derive(Clone, serde::Serialize)]
pub struct RegionInit {
    /// Absolute path of the frozen screenshot PNG (the FE `convertFileSrc`s it).
    pub path: String,
    /// Logical bounds of the target monitor (the window covers exactly this).
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Foreground-window UI element rects (monitor-local CSS) for the smart pick.
    pub elements: Vec<ElemRect>,
    /// Whole-window hover targets (virtual-desktop CSS px) for the `window` picker mode.
    pub windows: Vec<WinRect>,
    /// Whole-monitor hover targets (virtual-desktop CSS px) for the `display` picker mode.
    pub monitors: Vec<MonRect>,
    /// Which selection mode the overlay starts in (`rect` | `free` | `smart` | `window` |
    /// `display`) — set by the method the user picked on the mini toolbar. Falls back to
    /// `rect` if unknown.
    pub initial_mode: String,
}

static REGION_INIT: Mutex<Option<RegionInit>> = Mutex::new(None);

/// The outcome of a region selection, **pulled** by the Tyto window rather than
/// pushed. Pushing an event to a window that was hidden during selection and is only
/// just being re-shown is racy (a freshly-revealed WebView2 can miss it); an outgoing
/// `invoke` from that window always works, so the store polls this instead.
#[derive(Clone, serde::Serialize)]
pub struct RegionResult {
    /// `true` = confirmed with a rectangle; `false` = cancelled (leave the region as-is).
    pub confirmed: bool,
    /// CSS-pixel rectangle (window-local); only meaningful when `confirmed`.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Freehand polygon in **window-local CSS px** — present only on a freehand
    /// confirm, `None` for a plain rectangle. The Tyto window resolves it to physical
    /// region-local pixels and forwards it as a screenshot mask.
    pub points: Option<Vec<[i32; 2]>>,
    /// The picked whole-window id (`win-<hwnd>`) — set only when the user clicked a
    /// window in the `window` picker mode. `None` for every other outcome.
    pub window_id: Option<String>,
    /// The picked whole-monitor id (`mon-<hmonitor>`) — set only when the user clicked a
    /// monitor in the `display` picker mode. `None` for every other outcome.
    pub monitor_id: Option<String>,
}

static REGION_RESULT: Mutex<Option<RegionResult>> = Mutex::new(None);

fn set_region_result(result: Option<RegionResult>) {
    if let Ok(mut g) = REGION_RESULT.lock() {
        *g = result;
    }
}

/// Open the opaque region-selection overlay over `screenshot_path`, covering the
/// monitor at the given logical bounds. The Tyto window is already hidden by the
/// store (before it froze the desktop, so Tyto isn't in the frame) — this only
/// builds the overlay.
#[tauri::command]
#[allow(clippy::unused_async)] // async moves the handler off the main thread (WebView2 creation).
pub async fn open_region_selector_window(
    app: AppHandle,
    screenshot_path: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    elements: Vec<ElemRect>,
    initial_mode: String,
    windows: Vec<WinRect>,
    monitors: Vec<MonRect>,
) {
    if let Ok(mut g) = REGION_INIT.lock() {
        *g = Some(RegionInit {
            path: screenshot_path, x, y, width, height, elements, windows, monitors, initial_mode,
        });
    }
    // Clear any stale outcome so the store's poll only sees THIS selection's result.
    set_region_result(None);
    super::dispatch_to_main(&app, "tyto-region", move |app| build_region_window(app, x, y, width, height));
}

/// Main-thread builder: an opaque, borderless, always-on-top window at the
/// monitor's logical bounds. Reuses the shared WebView2 env.
fn build_region_window(app: &AppHandle, x: i32, y: i32, width: u32, height: u32) {
    if let Some(w) = app.get_webview_window(TYTO_REGION_LABEL) {
        // Reuse the existing (hidden) overlay instead of rebuilding a fresh WebView2
        // each time — building one (load index.html, mount Svelte, init theme) is what
        // made every open/close feel laggy. Resize/reposition it to the new frame's
        // bounds while still hidden, tell the FE to reload its init + frozen frame, and
        // let it reveal on the new image's `load` (or the armed fallback). REGION_INIT
        // was already set by `open_region_selector_window` before this runs.
        let _ = w.set_size(LogicalSize::new(width as f64, height as f64));
        let _ = w.set_position(LogicalPosition::new(x as f64, y as f64));
        let _ = w.emit(REGION_REINIT_EVENT, ());
        super::arm_ready_reveal(app, TYTO_REGION_LABEL);
        return;
    }
    let res = WebviewWindowBuilder::new(app, TYTO_REGION_LABEL, WebviewUrl::default())
        .title("Select region")
        .inner_size(width as f64, height as f64)
        .decorations(false)
        .transparent(false) // MUST stay opaque — see the module doc.
        .always_on_top(true)
        .shadow(false)
        .skip_taskbar(true)
        .resizable(false)
        // Build HIDDEN and let the FE reveal it once the frozen screenshot has painted
        // (the generic `window_ready`, called on the image's `load`). Otherwise the
        // opaque webview shows its default white page for the load beat — a visible
        // white flash before the frozen frame appears. See super::window_ready.
        .visible(false)
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();
    match res {
        Ok(w) => {
            // Position (while hidden) at the monitor's logical origin. Shown later by
            // `window_ready` (once the frozen frame paints — no white flash), or the
            // armed fallback below.
            let _ = w.set_position(LogicalPosition::new(x as f64, y as f64));
            super::arm_ready_reveal(app, TYTO_REGION_LABEL);
        }
        Err(e) => {
            tracing::error!("failed to open tyto region window: {e}");
            // Don't leave Tyto hidden if the overlay couldn't open.
            if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
                show_and_focus(&w);
            }
        }
    }
}

/// The region window pulls its init (screenshot + bounds) on mount.
#[tauri::command]
pub fn get_region_init() -> Option<RegionInit> {
    REGION_INIT.lock().ok().and_then(|g| g.clone())
}

/// Confirm a selection: record the CSS-pixel rect for the Tyto window to pull, then
/// close the overlay and restore Tyto (which resolves the rect via `select_region`).
///
/// `points` (window-local CSS px) is the freehand polygon — `None`/absent for a plain
/// rectangle, so the existing rect path stays backward compatible.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn region_selector_confirm(
    app: AppHandle,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    points: Option<Vec<[i32; 2]>>,
) {
    set_region_result(Some(RegionResult {
        confirmed: true, x, y, width, height, points, window_id: None, monitor_id: None,
    }));
    close_and_restore(&app);
}

/// Pick a whole window or whole monitor from the on-screen picker overlay: record the
/// chosen id (no rectangle) for the Tyto window to pull, then close + restore Tyto. `kind`
/// is `"window"` (→ `window_id`) or `"display"` (→ `monitor_id`); any other value records
/// neither (a no-op selection).
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn region_selector_pick(app: AppHandle, kind: String, id: String) {
    set_region_result(Some(RegionResult {
        confirmed: true,
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        points: None,
        window_id: (kind == "window").then(|| id.clone()),
        monitor_id: (kind == "display").then_some(id),
    }));
    close_and_restore(&app);
}

/// Cancel: record a "cancelled" outcome (so the store's poll stops without changing
/// the region), then close the overlay and restore Tyto.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn region_selector_cancel(app: AppHandle) {
    set_region_result(Some(RegionResult {
        confirmed: false, x: 0, y: 0, width: 0, height: 0, points: None, window_id: None, monitor_id: None,
    }));
    close_and_restore(&app);
}

/// The Tyto window polls this after opening the overlay. Returns `None` while the
/// user is still selecting, then the outcome exactly once (it's taken/cleared on read).
#[tauri::command]
pub fn take_region_result() -> Option<RegionResult> {
    REGION_RESULT.lock().ok().and_then(|mut g| g.take())
}

fn close_and_restore(app: &AppHandle) {
    if let Ok(mut g) = REGION_INIT.lock() {
        *g = None;
    }
    if let Some(w) = app.get_webview_window(TYTO_REGION_LABEL) {
        // HIDE, don't close: keep the WebView2 alive so the next selection reuses it
        // (see `build_region_window`) instead of paying a fresh window build.
        let _ = w.hide();
    }
    // Re-show Tyto: this un-throttles its background timers so the next poll returns
    // the result promptly.
    if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
        show_and_focus(&w);
    }
}
