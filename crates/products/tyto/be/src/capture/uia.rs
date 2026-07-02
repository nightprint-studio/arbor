//! UI Automation element enumeration for the **smart** region pick.
//!
//! The selection overlay is an opaque window that covers the monitor, so a *live*
//! `ElementFromPoint` at hover time would just return the overlay. Instead we snapshot
//! every UIA element's bounding rect from the **foreground window** BEFORE the overlay
//! opens; the overlay then hit-tests the cursor against these rects (smallest area
//! containing the cursor = the most specific element, e.g. a browser's content pane).
//!
//! Windows-only (returns empty elsewhere). Runs on a dedicated COM-initialized thread
//! with a timeout so a huge/slow tree can't hang the caller. API + performance were
//! verified against a standalone probe (~1300 elements cached in ~300ms via one
//! `FindAllBuildCache` call).

use std::sync::mpsc;
use std::time::Duration;

use crate::region::PixelRect;

/// Enumerate the foreground window's UIA element rects that fall on `monitor_id`,
/// converted to **monitor-local CSS pixels** (the overlay's coordinate space). Empty
/// on failure / non-Windows / timeout.
pub fn enumerate_elements(monitor_id: &str) -> Vec<PixelRect> {
    let mid = monitor_id.to_string();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(enumerate_on_com_thread(&mid));
    });
    rx.recv_timeout(Duration::from_secs(3)).unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn enumerate_on_com_thread(monitor_id: &str) -> Vec<PixelRect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, TreeScope_Subtree, UIA_BoundingRectanglePropertyId,
    };
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let id = match monitor_id.strip_prefix("mon-").and_then(|s| s.parse::<u32>().ok()) {
        Some(v) => v,
        None => return Vec::new(),
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Monitor physical rect + DPI scale (mirrors source.rs / gdi.rs).
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

        let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(a) => a,
            Err(_) => return Vec::new(),
        };
        let hwnd = GetForegroundWindow();
        let root = match automation.ElementFromHandle(hwnd) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let cache = match automation.CreateCacheRequest() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let _ = cache.AddProperty(UIA_BoundingRectanglePropertyId);
        let cond = match automation.CreateTrueCondition() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let all = match root.FindAllBuildCache(TreeScope_Subtree, &cond, &cache) {
            Ok(a) => a,
            Err(_) => return Vec::new(),
        };
        let n = all.Length().unwrap_or(0);

        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let e = match all.GetElement(i) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let r = e.CachedBoundingRectangle().unwrap_or_default();
            if r.right - r.left <= 0 || r.bottom - r.top <= 0 {
                continue;
            }
            // Keep only elements intersecting this monitor; clamp to it.
            if r.right <= m.left || r.left >= m.right || r.bottom <= m.top || r.top >= m.bottom {
                continue;
            }
            let l = r.left.max(m.left);
            let t = r.top.max(m.top);
            let rr = r.right.min(m.right);
            let bb = r.bottom.min(m.bottom);
            let cw = ((rr - l) as f64 / scale).round();
            let ch = ((bb - t) as f64 / scale).round();
            if cw < 1.0 || ch < 1.0 {
                continue;
            }
            out.push(PixelRect {
                x: (((l - m.left) as f64) / scale).round() as i32,
                y: (((t - m.top) as f64) / scale).round() as i32,
                w: cw as u32,
                h: ch as u32,
            });
        }
        out
    }
}

#[cfg(not(target_os = "windows"))]
fn enumerate_on_com_thread(_monitor_id: &str) -> Vec<PixelRect> {
    Vec::new()
}
