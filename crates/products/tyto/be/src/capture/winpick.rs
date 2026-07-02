//! Enumerate window hover-rects for the in-window fullscreen Snip **selector**.
//!
//! The selector covers ONE monitor with a frozen backdrop; this returns that monitor's
//! top-level windows in **monitor-local CSS pixels** (physical screen coords clipped to
//! the monitor, minus its origin, divided by its DPI scale) — the SAME space as
//! `region::freeze_screen` and [`super::uia::enumerate_elements`], so hover hit-tests
//! line up with the backdrop. Windows-only real impl; the non-Windows stub returns empty.
//! Ids match the existing `source.rs` formula: `win-<hwnd as u32>`.

use crate::region::WindowPickRect;

/// Enumerate top-level windows that fall on `monitor_id`, clipped to that monitor and
/// converted to **monitor-local CSS pixels** — the coordinate space of the in-window
/// fullscreen selector (matches [`super::uia::enumerate_elements`] /
/// `region::freeze_screen`). Empty off Windows / on lookup failure.
pub fn enumerate_windows_on_monitor(monitor_id: &str) -> Vec<WindowPickRect> {
    #[cfg(target_os = "windows")]
    {
        win_windows_on_monitor(monitor_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = monitor_id;
        Vec::new()
    }
}

/// True for a title that belongs to Tyto itself — never offer our own windows as a
/// pick target (mirrors `source::is_own_window`).
#[cfg(target_os = "windows")]
fn is_own_window(title: &str) -> bool {
    title.starts_with("Tyto —") || title == "Select region"
}

/// Top-level windows intersecting `monitor_id`, clipped to the monitor and converted to
/// monitor-local CSS px (mirrors `uia::enumerate_elements`'s monitor-scoped conversion).
#[cfg(target_os = "windows")]
fn win_windows_on_monitor(monitor_id: &str) -> Vec<WindowPickRect> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    use windows_capture::window::Window;

    let id = match monitor_id.strip_prefix("mon-").and_then(|s| s.parse::<u32>().ok()) {
        Some(v) => v,
        None => return Vec::new(),
    };

    unsafe {
        // Monitor physical rect + DPI scale (mirrors uia.rs / source.rs).
        let hmon = HMONITOR(id as usize as *mut std::ffi::c_void);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if !GetMonitorInfoW(hmon, &mut info).as_bool() {
            return Vec::new();
        }
        let m = info.rcMonitor;
        let (mut dx, mut dy) = (96u32, 96u32);
        let scale = if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() {
            (dx as f64 / 96.0).max(0.1)
        } else {
            1.0
        };

        // Window::enumerate already drops invisible / tool / child windows.
        let wins = match Window::enumerate() {
            Ok(w) => w,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for w in wins {
            let title = w.title().unwrap_or_default();
            if title.trim().is_empty() || is_own_window(&title) {
                continue;
            }
            let hwnd_ptr = w.as_raw_hwnd();
            let id = hwnd_ptr as usize as u32;
            let mut rect = RECT::default();
            // GetWindowRect gives the physical screen bounds (including the frame).
            if GetWindowRect(HWND(hwnd_ptr), &mut rect).is_err() {
                continue;
            }
            // Keep only windows intersecting this monitor; clip to it.
            if rect.right <= m.left || rect.left >= m.right || rect.bottom <= m.top || rect.top >= m.bottom {
                continue;
            }
            let l = rect.left.max(m.left);
            let t = rect.top.max(m.top);
            let rr = rect.right.min(m.right);
            let bb = rect.bottom.min(m.bottom);
            let cw = ((rr - l) as f64 / scale).round();
            let ch = ((bb - t) as f64 / scale).round();
            if cw < 1.0 || ch < 1.0 {
                continue;
            }
            out.push(WindowPickRect {
                id: format!("win-{id}"),
                x: (((l - m.left) as f64) / scale).round() as i32,
                y: (((t - m.top) as f64) / scale).round() as i32,
                w: cw as u32,
                h: ch as u32,
            });
        }
        out
    }
}
