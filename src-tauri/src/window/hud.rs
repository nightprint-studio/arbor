//! The **recording HUD** — a small, opaque, always-on-top control surface shown
//! while a video recording runs *with the Tyto window hidden* (so Tyto's own UI is
//! never in the shot). It has two layouts the user toggles at will:
//! - **compact** — a slim Windows-style pill (REC · elapsed · pause · stop),
//! - **expanded** — a card that also shows what's being recorded and larger controls.
//!
//! The FE asks the shell to [`resize_recording_hud`] when the user toggles layout;
//! the shell owns the size + top-center placement so the positioning logic lives in
//! one spot — on **the display being recorded**, which the FE names when it knows it
//! (see [`HudDisplay`]). The window pulls its [`get_hud_init`] (the target label) on
//! mount to avoid an emit/listen race.
//!
//! It is marked **content-protected** (`SetWindowDisplayAffinity` /
//! `WDA_EXCLUDEFROMCAPTURE` under the hood) so the HUD itself doesn't show up in the
//! recording — effective as long as the capture backend honors display affinity.
//!
//! OPAQUE, never transparent: a transparent WebView2 window receives no input on
//! Windows (documented Arbor trap) and the controls must stay clickable — so this is
//! a solid floating bar, not a see-through overlay.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

use super::{show_and_focus, WEBVIEW_BROWSER_ARGS};

/// Label for the recording HUD window. Matches the `tyto-*` capability glob so it
/// inherits the Tyto window's permissions (event listen, window controls).
pub const TYTO_HUD_LABEL: &str = "tyto-hud";

const HUD_COMPACT_W: f64 = 328.0;
const HUD_COMPACT_H: f64 = 52.0;
const HUD_EXPANDED_W: f64 = 300.0;
const HUD_EXPANDED_H: f64 = 158.0;

/// What the HUD window pulls on mount (avoids an emit/listen race with a just-created
/// webview). Set by [`open_recording_hud`] before the window is built.
#[derive(Clone, Default, serde::Serialize)]
pub struct HudInit {
    /// Human label of what's being recorded (e.g. a monitor or window name).
    pub target_label: String,
}

/// Which display the HUD belongs on, as a rectangle in **physical desktop pixels** —
/// the one the frontend read from `list_capture_sources` for the capture target.
///
/// `None` when the frontend can't tell (a window picked from the panel's grid names no
/// display), and then the HUD is placed wherever it already is. The primary monitor is
/// the wrong answer either way: a recording of the second screen put its own controls
/// on the first one, where the person watching the recording wasn't looking.
#[derive(Clone, Copy, serde::Deserialize)]
pub struct HudDisplay {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

static HUD_INIT: Mutex<Option<HudInit>> = Mutex::new(None);
/// Set by [`open_recording_hud`], read by every placement until the HUD closes.
static HUD_DISPLAY: Mutex<Option<HudDisplay>> = Mutex::new(None);

/// How far below the top of the display's work area the HUD sits, in logical pixels.
/// Measured from the **work area** rather than the display: on macOS the menu bar owns
/// the strip above it, and a HUD at a flat offset from the display top disappears
/// behind it (further still on a notched screen).
const HUD_TOP_GAP: f64 = 24.0;

/// Open the recording HUD and hide the Tyto window: during a recording the HUD is
/// the on-screen control, and Tyto stays out of the capture. `target_label` is what
/// the expanded HUD shows as the recording subject.
#[tauri::command]
#[allow(clippy::unused_async)] // async moves the handler off the main thread (WebView2 creation).
pub async fn open_recording_hud(app: AppHandle, target_label: String, display: Option<HudDisplay>) {
    if let Ok(mut g) = HUD_INIT.lock() {
        *g = Some(HudInit { target_label });
    }
    if let Ok(mut g) = HUD_DISPLAY.lock() {
        *g = display;
    }
    if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
        let _ = w.hide();
    }
    super::dispatch_to_main(&app, "tyto-hud", build_hud_window);
}

/// The HUD window pulls its init (target label) on mount.
#[tauri::command]
pub fn get_hud_init() -> HudInit {
    HUD_INIT.lock().ok().and_then(|g| g.clone()).unwrap_or_default()
}

/// Position the HUD at the top-center of the display being recorded, for the given
/// logical width.
///
/// The display is [`HUD_DISPLAY`] when the frontend named one, else the one the HUD is
/// already on, else the primary — in that order, so a recording of the second screen
/// keeps its controls there while a target that names no display doesn't get moved
/// somewhere arbitrary. The maths is done in physical pixels and handed to
/// [`super::place_physical`], which knows what each platform wants.
fn place_top_center(w: &tauri::WebviewWindow, width: f64) {
    let named = HUD_DISPLAY.lock().ok().and_then(|g| *g);
    let mon = named
        .and_then(|r| super::monitor_at_physical(w, r.x, r.y))
        .or_else(|| w.current_monitor().ok().flatten())
        .or_else(|| w.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return };
    let scale = mon.scale_factor().max(0.1);
    let area = mon.work_area();
    let x = area.position.x + (((area.size.width as f64 - width * scale) / 2.0).max(0.0) as i32);
    let y = area.position.y + (HUD_TOP_GAP * scale) as i32;
    super::place_physical(w, x, y, None);
}

/// Main-thread builder: an opaque, content-protected, always-on-top pill at the top
/// center of the display being recorded (compact layout by default).
fn build_hud_window(app: &AppHandle) {
    // Recording is now live (the HUD is the on-screen control): let the global-shortcut
    // handler know a press should STOP rather than open a new selector.
    super::tyto::set_recording(true);
    if let Some(w) = app.get_webview_window(TYTO_HUD_LABEL) {
        show_and_focus(&w);
        return;
    }
    let res = WebviewWindowBuilder::new(app, TYTO_HUD_LABEL, WebviewUrl::default())
        .title("Tyto — Recording")
        .inner_size(HUD_COMPACT_W, HUD_COMPACT_H)
        .decorations(false)
        // .transparent(false) // MUST stay opaque — see the module doc.
        .always_on_top(true)
        .content_protected(true) // exclude the HUD from screen capture where honored.
        .shadow(true)
        .skip_taskbar(true)
        .resizable(false)
        // Build HIDDEN and let the FE reveal it once it has painted (the generic
        // `window_ready`), so the opaque webview's default white page never flashes
        // before the bar. See super::window_ready / arm_ready_reveal.
        .visible(false)
        .additional_browser_args(WEBVIEW_BROWSER_ARGS)
        .build();
    match res {
        Ok(w) => {
            // Position while hidden; shown by `window_ready` (or the armed fallback).
            place_top_center(&w, HUD_COMPACT_W);
            super::arm_ready_reveal(app, TYTO_HUD_LABEL);
        }
        Err(e) => {
            tracing::error!("failed to open tyto recording hud: {e}");
            super::tyto::set_recording(false);
            // Don't leave Tyto hidden if the HUD couldn't open.
            if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
                super::tyto::reset_to_full_panel(&w);
                show_and_focus(&w);
            }
        }
    }
}

/// Resize the HUD between its compact and expanded layouts, keeping it pinned to the
/// top-center of the same display. Driven by the FE's layout toggle.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn resize_recording_hud(app: AppHandle, expanded: bool) {
    if let Some(w) = app.get_webview_window(TYTO_HUD_LABEL) {
        let (width, height) = if expanded {
            (HUD_EXPANDED_W, HUD_EXPANDED_H)
        } else {
            (HUD_COMPACT_W, HUD_COMPACT_H)
        };
        let _ = w.set_size(LogicalSize::new(width, height));
        place_top_center(&w, width);
    }
}

/// Close the HUD, restore the Tyto window, and tell it the recording stopped. The
/// short delayed emit mirrors the region window: a just-shown WebView2 can miss an
/// event fired in the same tick, so we hand it to a short-lived thread.
#[tauri::command]
#[allow(clippy::unused_async)]
pub async fn close_recording_hud(app: AppHandle) {
    if let Ok(mut g) = HUD_INIT.lock() {
        *g = None;
    }
    if let Ok(mut g) = HUD_DISPLAY.lock() {
        *g = None;
    }
    super::tyto::set_recording(false);
    // A finished recording ends with Tyto's panel in front, showing the new capture —
    // so the application the quick capture interrupted is no longer owed its turn.
    super::mac_overlay::forget_frontmost_app();
    if let Some(w) = app.get_webview_window(TYTO_HUD_LABEL) {
        let _ = w.close();
    }
    if let Some(w) = app.get_webview_window(super::tyto::TYTO_WINDOW_LABEL) {
        // A countdown-started recording left Tyto at its monitor-covering "countdown"
        // bounds (hidden during the recording) — restore the normal panel geometry before
        // revealing it. Idempotent for recordings that never covered.
        super::tyto::reset_to_full_panel(&w);
        show_and_focus(&w);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(140));
        let _ = app.emit_to(
            super::tyto::TYTO_WINDOW_LABEL,
            "tyto://recording-stopped",
            serde_json::json!({}),
        );
    });
}
