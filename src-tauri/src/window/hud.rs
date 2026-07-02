//! The **recording HUD** — a small, opaque, always-on-top control surface shown
//! while a video recording runs *with the Tyto window hidden* (so Tyto's own UI is
//! never in the shot). It has two layouts the user toggles at will:
//! - **compact** — a slim Windows-style pill (REC · elapsed · pause · stop),
//! - **expanded** — a card that also shows what's being recorded and larger controls.
//!
//! The FE asks the shell to [`resize_recording_hud`] when the user toggles layout;
//! the shell owns the size + top-center placement so the positioning logic lives in
//! one spot. The window pulls its [`get_hud_init`] (the target label) on mount to
//! avoid an emit/listen race.
//!
//! It is marked **content-protected** (`SetWindowDisplayAffinity` /
//! `WDA_EXCLUDEFROMCAPTURE` under the hood) so the HUD itself doesn't show up in the
//! recording — effective as long as the capture backend honors display affinity.
//!
//! OPAQUE, never transparent: a transparent WebView2 window receives no input on
//! Windows (documented Arbor trap) and the controls must stay clickable — so this is
//! a solid floating bar, not a see-through overlay.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindowBuilder};

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

static HUD_INIT: Mutex<Option<HudInit>> = Mutex::new(None);

/// Open the recording HUD and hide the Tyto window: during a recording the HUD is
/// the on-screen control, and Tyto stays out of the capture. `target_label` is what
/// the expanded HUD shows as the recording subject.
#[tauri::command]
#[allow(clippy::unused_async)] // async moves the handler off the main thread (WebView2 creation).
pub async fn open_recording_hud(app: AppHandle, target_label: String) {
    if let Ok(mut g) = HUD_INIT.lock() {
        *g = Some(HudInit { target_label });
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

/// Position the HUD at the top-center of the primary monitor for the given size.
fn place_top_center(w: &tauri::WebviewWindow, width: f64) {
    if let Ok(Some(mon)) = w.primary_monitor() {
        let scale = mon.scale_factor();
        let logical_w = mon.size().width as f64 / scale;
        let x = ((logical_w - width) / 2.0).max(0.0);
        let _ = w.set_position(LogicalPosition::new(x, 24.0));
    }
}

/// Main-thread builder: an opaque, content-protected, always-on-top pill at the top
/// center of the primary monitor (compact layout by default).
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
        .transparent(false) // MUST stay opaque — see the module doc.
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
/// top-center of the primary monitor. Driven by the FE's layout toggle.
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
    super::tyto::set_recording(false);
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
