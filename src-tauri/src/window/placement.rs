//! Launcher placement — park the `main` (Canopy launcher) window in the
//! bottom-right of the work area, JetBrains-Toolbox-style.
//!
//! On Windows the work area is read via `SPI_GETWORKAREA` (excludes the
//! taskbar) so the launcher sits just above it; elsewhere we fall back to the
//! current monitor's bounds minus a conservative bottom margin.

use tauri::{AppHandle, Manager, PhysicalPosition};

/// Gap (physical px) between the launcher and the work-area edges.
const MARGIN: i32 = 18;

/// Position the `main` launcher window at the bottom-right of the work area.
/// No-op (logged) if the window or monitor info isn't available yet.
pub fn place_launcher_bottom_right(app: &AppHandle) {
    let Some(w) = app.get_webview_window("main") else { return };
    let size = match w.outer_size() {
        Ok(s) => s,
        Err(e) => { tracing::warn!("launcher placement: outer_size failed: {e}"); return; }
    };
    let win_w = size.width as i32;
    let win_h = size.height as i32;

    let Some((left, top, right, bottom)) = work_area(&w) else { return };
    let x = (right - win_w - MARGIN).max(left + MARGIN);
    let y = (bottom - win_h - MARGIN).max(top + MARGIN);
    if let Err(e) = w.set_position(PhysicalPosition::new(x, y)) {
        tracing::warn!("launcher placement: set_position failed: {e}");
    }
}

/// Work area (physical px: left, top, right, bottom). Windows uses
/// `SPI_GETWORKAREA`; other platforms approximate from the monitor bounds.
#[cfg(windows)]
fn work_area(_w: &tauri::WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA};
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: SPI_GETWORKAREA writes a RECT into `pvParam`; the pointer is a
    // valid, correctly-sized, exclusively-borrowed RECT for the call's duration.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            (&mut rect as *mut RECT).cast(),
            0,
        )
    };
    if ok == 0 {
        tracing::warn!("launcher placement: SPI_GETWORKAREA failed");
        return None;
    }
    Some((rect.left, rect.top, rect.right, rect.bottom))
}

#[cfg(not(windows))]
fn work_area(w: &tauri::WebviewWindow) -> Option<(i32, i32, i32, i32)> {
    // No portable work-area query — approximate from the monitor bounds and
    // leave room at the bottom for a typical panel/dock.
    const DOCK_GUESS: i32 = 56;
    let mon = w.current_monitor().ok().flatten()?;
    let pos = mon.position();
    let size = mon.size();
    let left = pos.x;
    let top = pos.y;
    let right = pos.x + size.width as i32;
    let bottom = pos.y + size.height as i32 - DOCK_GUESS;
    Some((left, top, right, bottom))
}
