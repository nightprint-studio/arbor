//! Enumerate whole-window and whole-monitor pick targets for the on-screen
//! **window/display picker** overlay. The overlay is a frozen still of the ENTIRE
//! virtual desktop (see [`super::gdi::capture_virtual_desktop_rgba`]); the user
//! hovers a window or a monitor (blue outline) and clicks. This module produces the
//! hover-target rectangles, in the overlay's coordinate space.
//!
//! Coordinate space: the overlay spans the whole virtual desktop. Rects are returned
//! in **virtual-desktop CSS pixels** — physical screen coords minus the virtual-screen
//! origin, divided by the PRIMARY monitor's DPI scale.
//!
//! ⚠ Assumption: **uniform DPI across monitors** — we convert every rect with the
//! primary monitor's scale. Mixed-DPI multi-monitor setups will have mis-sized rects
//! on the non-primary monitors; that's a known limitation.
//!
//! Windows-only real impl (`#[cfg(target_os = "windows")]`); the non-Windows stub
//! returns empty. Ids match the existing `source.rs` formulas exactly:
//! `win-<hwnd as u32>` / `mon-<hmonitor as u32>`.

use crate::region::{MonitorPickRect, PickTargets, WindowPickRect};

/// Enumerate top-level windows + monitors as pick targets in virtual-desktop CSS
/// pixels. Empty off Windows.
pub fn enumerate_pick_targets() -> PickTargets {
    #[cfg(target_os = "windows")]
    {
        win_pick_targets()
    }
    #[cfg(not(target_os = "windows"))]
    {
        PickTargets { windows: Vec::new(), monitors: Vec::new() }
    }
}

/// True for a title that belongs to Tyto itself — never offer our own windows as a
/// pick target (mirrors `source::is_own_window`).
#[cfg(target_os = "windows")]
fn is_own_window(title: &str) -> bool {
    title.starts_with("Tyto —") || title == "Select region"
}

#[cfg(target_os = "windows")]
fn win_pick_targets() -> PickTargets {
    let geom = virtual_desktop_geometry();
    PickTargets {
        windows: win_windows(&geom),
        monitors: win_monitors(&geom),
    }
}

/// Virtual-desktop bounds (physical, from GetSystemMetrics) + the primary monitor's
/// DPI scale used for the whole conversion.
#[cfg(target_os = "windows")]
pub(crate) struct VirtualGeometry {
    pub left: i32,
    pub top: i32,
    pub width: i32,
    pub height: i32,
    pub scale: f64,
}

#[cfg(target_os = "windows")]
impl VirtualGeometry {
    /// Physical screen coords → virtual-desktop CSS px (origin-subtracted, scaled).
    fn to_css(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, u32, u32) {
        let cx = (((x - self.left) as f64) / self.scale).round() as i32;
        let cy = (((y - self.top) as f64) / self.scale).round() as i32;
        let cw = ((w.max(0) as f64) / self.scale).round().max(0.0) as u32;
        let ch = ((h.max(0) as f64) / self.scale).round().max(0.0) as u32;
        (cx, cy, cw, ch)
    }
}

/// Virtual-screen bounds via GetSystemMetrics + the primary monitor's DPI scale.
#[cfg(target_os = "windows")]
pub(crate) fn virtual_desktop_geometry() -> VirtualGeometry {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        VirtualGeometry { left, top, width, height, scale: primary_scale() }
    }
}

/// DPI scale of the PRIMARY monitor (uniform-DPI assumption). Falls back to 1.0.
#[cfg(target_os = "windows")]
fn primary_scale() -> f64 {
    use windows_capture::monitor::Monitor;

    let scale = Monitor::primary().ok().map(|m| {
        let id = m.as_raw_hmonitor() as usize as u32;
        crate::capture::source::monitor_geometry(&format!("mon-{id}")).2
    });
    scale.unwrap_or(1.0).max(0.1)
}

#[cfg(target_os = "windows")]
fn win_windows(geom: &VirtualGeometry) -> Vec<WindowPickRect> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    use windows_capture::window::Window;

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
        let ok = unsafe { GetWindowRect(HWND(hwnd_ptr), &mut rect).is_ok() };
        if !ok {
            continue;
        }
        let pw = rect.right - rect.left;
        let ph = rect.bottom - rect.top;
        if pw <= 0 || ph <= 0 {
            continue;
        }
        let (x, y, cw, ch) = geom.to_css(rect.left, rect.top, pw, ph);
        if cw == 0 || ch == 0 {
            continue;
        }
        out.push(WindowPickRect { id: format!("win-{id}"), x, y, w: cw, h: ch });
    }
    out
}

#[cfg(target_os = "windows")]
fn win_monitors(geom: &VirtualGeometry) -> Vec<MonitorPickRect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows_capture::monitor::Monitor;

    let mons = match Monitor::enumerate() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for m in mons {
        let raw = m.as_raw_hmonitor() as usize;
        let id = raw as u32;
        let name = m
            .name()
            .or_else(|_| m.device_name())
            .unwrap_or_else(|_| format!("Display {id}"));

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        let hmon = HMONITOR(raw as *mut std::ffi::c_void);
        let ok = unsafe { GetMonitorInfoW(hmon, &mut info).as_bool() };
        if !ok {
            continue;
        }
        let r = info.rcMonitor;
        let (x, y, cw, ch) = geom.to_css(r.left, r.top, r.right - r.left, r.bottom - r.top);
        if cw == 0 || ch == 0 {
            continue;
        }
        out.push(MonitorPickRect { id: format!("mon-{id}"), name, x, y, w: cw, h: ch });
    }
    out
}
