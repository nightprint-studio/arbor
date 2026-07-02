//! GDI monitor capture for the region-selector **freeze** — a single `BitBlt` of the
//! target monitor. Unlike a scap/WGC capture session it opens no capture session, so
//! Windows shows **no yellow "you are being captured" border** (the freeze is only a
//! still backdrop for the selection overlay; the real recording still uses scap).
//!
//! Windows-only; the freeze falls back to scap elsewhere. The exact `windows 0.61`
//! GDI API here was verified against a standalone probe (BitBlt + GetDIBits +
//! SelectObject + `BI_RGB.0 as u32`), and the `GetMonitorInfoW` idiom mirrors
//! [`super::source`]'s `win_monitor_geometry`.

use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
    DIB_RGB_COLORS, HGDIOBJ, HMONITOR, MONITORINFO, SRCCOPY,
};

/// Capture the monitor `mon-<id>` via GDI → `(rgba, width, height)` in physical pixels.
/// Border-free. Errors if the id is malformed or any GDI step fails.
pub fn capture_monitor_rgba(monitor_id: &str) -> Result<(Vec<u8>, u32, u32), String> {
    let id = monitor_id
        .strip_prefix("mon-")
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| "GDI capture: bad monitor id".to_string())?;

    let hmon = HMONITOR(id as usize as *mut std::ffi::c_void);
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT::default(),
        rcWork: RECT::default(),
        dwFlags: 0,
    };
    if !unsafe { GetMonitorInfoW(hmon, &mut info).as_bool() } {
        return Err("GDI capture: GetMonitorInfoW failed".to_string());
    }
    let RECT { left, top, right, bottom } = info.rcMonitor;
    capture_rect_rgba(left, top, (right - left).max(1), (bottom - top).max(1))
}

/// Capture the WHOLE virtual desktop (all monitors, union rect) via GDI →
/// `(rgba, width, height)` in physical pixels. Border-free. The source rect comes
/// from the `SM_*VIRTUALSCREEN` metrics; `GetDC(None)` already spans the virtual
/// desktop so a single BitBlt from `(left, top)` grabs it all.
pub fn capture_virtual_desktop_rgba() -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        capture_rect_rgba(left, top, w, h)
    }
}

/// BitBlt a physical-screen rectangle (`left`/`top`/`w`/`h`) off the desktop DC into
/// an RGBA buffer. `GetDC(None)` returns the virtual-desktop DC, so `left`/`top` may
/// be negative (a monitor left of / above the primary). `w`/`h` must be ≥ 1.
pub fn capture_rect_rgba(left: i32, top: i32, w: i32, h: i32) -> Result<(Vec<u8>, u32, u32), String> {
    let w = w.max(1);
    let h = h.max(1);
    unsafe {
        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return Err("GDI capture: GetDC failed".to_string());
        }
        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        let hbmp = CreateCompatibleBitmap(hdc_screen, w, h);
        let old = SelectObject(hdc_mem, HGDIOBJ(hbmp.0));
        // CAPTUREBLT so layered/topmost windows are included in the still.
        let blt = BitBlt(hdc_mem, 0, 0, w, h, Some(hdc_screen), left, top, SRCCOPY | CAPTUREBLT);

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negative = top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let lines = GetDIBits(
            hdc_mem,
            hbmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Release GDI resources regardless of success.
        SelectObject(hdc_mem, old);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        if blt.is_err() || lines == 0 {
            return Err("GDI capture: BitBlt/GetDIBits failed".to_string());
        }

        // GDI gives BGRA (32-bit BI_RGB); swap B/R to RGBA and force opaque alpha
        // (GDI leaves alpha unspecified).
        let mut i = 0;
        while i + 3 < buf.len() {
            buf.swap(i, i + 2);
            buf[i + 3] = 255;
            i += 4;
        }
        Ok((buf, w as u32, h as u32))
    }
}
