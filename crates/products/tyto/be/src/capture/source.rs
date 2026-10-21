//! Real enumeration of capture targets. Backs the `sources` domain handlers,
//! replacing the frontend mock fixtures. Per-item errors are swallowed (skip the
//! bad monitor/window/device) so one failure never empties the whole picker.
//!
//! On Windows the rich metadata (monitor name/primary, window title + app name) comes
//! from `windows-capture` (the crate scap uses under the hood) plus a small Win32
//! process-name helper, joined to scap's capture side by the shared u32 handle id;
//! display geometry comes from Win32 itself. On macOS all of it comes from
//! CoreGraphics ([`super::mac`]), joined to scap by the display id / window number.
//! Elsewhere it falls back to scap's own (much thinner) enumeration.

use cpal::traits::{DeviceTrait, HostTrait};

use crate::sources::{AudioInput, CaptureSources, MonitorSource, WindowSource};

use super::access;

/// Monitors as wire structs (`mon-<id>` ids the FE hands back on select).
pub fn list_monitors() -> Vec<MonitorSource> {
    #[cfg(target_os = "windows")]
    {
        win_monitors()
    }
    #[cfg(target_os = "macos")]
    {
        mac_monitors()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        scap_monitors()
    }
}

/// Capturable windows (`win-<id>`).
pub fn list_windows() -> Vec<WindowSource> {
    #[cfg(target_os = "windows")]
    {
        win_windows()
    }
    #[cfg(target_os = "macos")]
    {
        mac_windows()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        scap_windows()
    }
}

/// True for a title that belongs to Tyto itself (main window, recording HUD, region
/// overlay) — never offer our own windows as a capture target.
fn is_own_window(title: &str) -> bool {
    title.starts_with("Tyto —") || title == "Select region"
}

// ── Windows: rich enumeration via windows-capture + a Win32 app-name helper ──

#[cfg(target_os = "windows")]
fn win_monitors() -> Vec<MonitorSource> {
    use windows_capture::monitor::Monitor;

    let primary = Monitor::primary().ok().map(|m| m.as_raw_hmonitor() as usize);
    let mons = match Monitor::enumerate() {
        Ok(m) => m,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for m in mons {
        let id = m.as_raw_hmonitor() as usize as u32;
        let name = m.name().or_else(|_| m.device_name()).unwrap_or_else(|_| format!("Display {id}"));
        let primary = Some(m.as_raw_hmonitor() as usize) == primary;
        // windows-capture has the size but neither the origin nor the DPI scale, and
        // the shell needs both to place a window on this display — so the whole
        // rectangle comes from Win32 ([`win_monitor_rect`]) and `windows-capture`
        // keeps only the job it is better at, naming the thing.
        let r = win_monitor_rect(id);
        out.push(MonitorSource {
            id: format!("mon-{id}"),
            name,
            x: r.map(|r| r.x).unwrap_or(0),
            y: r.map(|r| r.y).unwrap_or(0),
            width: r.map(|r| r.w).unwrap_or_else(|| m.width().unwrap_or(0)),
            height: r.map(|r| r.h).unwrap_or_else(|| m.height().unwrap_or(0)),
            scale: r.map(|r| r.scale).unwrap_or(1.0),
            primary,
        });
    }
    out
}

#[cfg(target_os = "windows")]
fn win_windows() -> Vec<WindowSource> {
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
        let id = w.as_raw_hwnd() as usize as u32;
        let app = app_name(w.as_raw_hwnd());
        out.push(WindowSource { id: format!("win-{id}"), title, app });
    }
    out
}

/// Window → owning process file stem (e.g. "Code", "firefox"), for app grouping.
#[cfg(target_os = "windows")]
fn app_name(hwnd_ptr: *mut std::ffi::c_void) -> String {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HWND};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    unsafe {
        let hwnd = HWND(hwnd_ptr);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return String::new();
        }
        let handle = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(_) => return String::new(),
        };
        let mut buf = [0u16; 260];
        let mut len = buf.len() as u32;
        let res = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        if res.is_err() || len == 0 {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let file = path.rsplit(['\\', '/']).next().unwrap_or(&path);
        file.strip_suffix(".exe")
            .or_else(|| file.strip_suffix(".EXE"))
            .unwrap_or(file)
            .to_string()
    }
}

// ── Monitor geometry ─────────────────────────────────────────────────────────

/// Where a monitor is and how dense it is: the rectangle in **physical pixels** on
/// the desktop, plus the DPI scale that maps it back to logical units.
///
/// Both halves are load-bearing and for different reasons. The **scale** is what maps
/// a CSS-pixel drag onto captured pixels, so a wrong one crops the wrong part of a
/// Retina display. The **origin** is what tells the shell which display to put the
/// selection overlay on, so a wrong one opens the overlay somewhere else entirely —
/// which is exactly what a hard-coded `(0, 0, 1.0)` did on every platform but Windows.
#[derive(Clone, Copy, Debug)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: f64,
}

/// Geometry of a monitor by `mon-<id>`. `None` when the id doesn't parse, the monitor
/// has been unplugged, or the platform has no implementation.
pub fn monitor_rect(monitor_id: &str) -> Option<MonitorRect> {
    let id = monitor_id.strip_prefix("mon-")?.parse::<u32>().ok()?;
    #[cfg(target_os = "windows")]
    {
        win_monitor_rect(id)
    }
    #[cfg(target_os = "macos")]
    {
        super::mac::display(id).map(|d| MonitorRect { x: d.x, y: d.y, w: d.w, h: d.h, scale: d.scale })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = id;
        None
    }
}

/// The DPI scale of a monitor, or `1.0` when it can't be determined — the one caller
/// that needs the scale alone ([`crate::region::select_region`]) would rather crop
/// 1:1 than not at all.
pub fn monitor_scale(monitor_id: &str) -> f64 {
    monitor_rect(monitor_id).map(|r| r.scale).unwrap_or(1.0)
}

/// `mon-<id>` of the monitor the **pointer** is on — the display a quick capture
/// means, whichever window happened to have focus. `None` where the platform can't
/// say, which leaves the caller on its own default (the primary).
pub fn monitor_at_cursor() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        win_monitor_at_cursor()
    }
    #[cfg(target_os = "macos")]
    {
        super::mac::display_at_cursor().map(|d| format!("mon-{}", d.id))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

#[cfg(target_os = "windows")]
fn win_monitor_rect(id: u32) -> Option<MonitorRect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    unsafe {
        let hmon = HMONITOR(id as usize as *mut std::ffi::c_void);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if !GetMonitorInfoW(hmon, &mut info).as_bool() {
            return None;
        }
        let r = info.rcMonitor;
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let scale = if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() {
            dpi_x as f64 / 96.0
        } else {
            1.0
        };
        Some(MonitorRect {
            x: r.left,
            y: r.top,
            w: (r.right - r.left).max(0) as u32,
            h: (r.bottom - r.top).max(0) as u32,
            scale,
        })
    }
}

#[cfg(target_os = "windows")]
fn win_monitor_at_cursor() -> Option<String> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_err() {
            return None;
        }
        let hmon = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        if hmon.0.is_null() {
            return None;
        }
        Some(format!("mon-{}", hmon.0 as usize as u32))
    }
}

// ── macOS: CoreGraphics geometry + scap's localized display names ────────────

/// Displays from CoreGraphics (position, size, scale, primary — none of which scap
/// exposes), named from scap's enumeration, which reads `NSScreen.localizedName` and
/// so says "DELL U2723QE" where CoreGraphics alone could only offer a vendor number.
/// A display scap doesn't name still appears, numbered.
#[cfg(target_os = "macos")]
fn mac_monitors() -> Vec<MonitorSource> {
    let names = mac_display_names();
    super::mac::displays()
        .into_iter()
        .enumerate()
        .map(|(i, d)| MonitorSource {
            id: format!("mon-{}", d.id),
            name: names
                .iter()
                .find(|(id, _)| *id == d.id)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| format!("Display {}", i + 1)),
            x: d.x,
            y: d.y,
            width: d.w,
            height: d.h,
            scale: d.scale,
            primary: d.primary,
        })
        .collect()
}

/// `(CGDirectDisplayID, localized name)` pairs from scap. Empty when capture is
/// refused — the names are a nicety, and [`mac_monitors`] falls back to numbering.
#[cfg(target_os = "macos")]
fn mac_display_names() -> Vec<(u32, String)> {
    scap_targets()
        .into_iter()
        .filter_map(|t| match t {
            scap::Target::Display(d) => Some((d.id, d.title)),
            _ => None,
        })
        .collect()
}

/// Windows from CoreGraphics — filtered to the ones a person would recognise and
/// carrying the owning application's name. See [`super::mac::windows`] for why
/// scap's own list can't be used here.
#[cfg(target_os = "macos")]
fn mac_windows() -> Vec<WindowSource> {
    super::mac::windows()
        .into_iter()
        .filter(|w| !is_own_window(&w.title))
        .map(|w| WindowSource { id: format!("win-{}", w.id), title: w.title, app: w.app })
        .collect()
}

/// Which display a window sits on, and the part of that display it covers, in
/// **physical display-local pixels** — the rectangle a region capture crops.
///
/// This is how a window gets captured on macOS at all: scap 0.0.8 sizes a window
/// stream through `NSApp windowWithWindowNumber:`, which only ever finds *our own*
/// windows, so any other application's window resolves to a 0×0 stream and the
/// capture comes back empty. Capturing the display it is on and cropping to its
/// frame produces the pixels instead.
///
/// The trade-off, and it is real: the crop is where the window was when the capture
/// started, and anything stacked on top of it is in the frame. For a preview or a
/// screenshot that is what the user sees anyway; a recording of a window that then
/// moves will keep filming the rectangle it left behind.
#[cfg(target_os = "macos")]
pub fn window_crop(window_id: &str) -> Option<(String, super::CropRect)> {
    let id = window_id.strip_prefix("win-")?.parse::<u32>().ok()?;
    let w = super::mac::window(id)?;
    // The window's centre decides the display, so a window straddling two monitors
    // is captured from the one it mostly sits on rather than failing.
    let (cx, cy) = (w.x + w.w / 2.0, w.y + w.h / 2.0);
    let d = super::mac::displays().into_iter().find(|d| {
        let (x, y) = (d.x as f64 / d.scale, d.y as f64 / d.scale);
        cx >= x && cx < x + d.w as f64 / d.scale && cy >= y && cy < y + d.h as f64 / d.scale
    })?;
    let s = d.scale;
    let local_x = ((w.x - d.x as f64 / s) * s).round();
    let local_y = ((w.y - d.y as f64 / s) * s).round();
    Some((
        format!("mon-{}", d.id),
        super::CropRect {
            x: local_x.max(0.0) as i32,
            y: local_y.max(0.0) as i32,
            w: (w.w * s).round().max(1.0) as u32,
            h: (w.h * s).round().max(1.0) as u32,
        },
    ))
}

// ── Non-Windows, non-macOS: scap's own (thinner) enumeration ─────────────────

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn scap_monitors() -> Vec<MonitorSource> {
    scap_targets()
        .into_iter()
        .filter_map(|t| match t {
            scap::Target::Display(d) => Some(MonitorSource {
                id: format!("mon-{}", d.id),
                name: d.title,
                // scap exposes neither geometry nor scale: a zero rectangle reads as
                // "unknown" to the shell, which then falls back to its own placement.
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                scale: 1.0,
                primary: false,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn scap_windows() -> Vec<WindowSource> {
    scap_targets()
        .into_iter()
        .filter_map(|t| match t {
            scap::Target::Window(w) if !w.title.trim().is_empty() && !is_own_window(&w.title) => {
                Some(WindowSource { id: format!("win-{}", w.id), title: w.title, app: String::new() })
            }
            _ => None,
        })
        .collect()
}

/// scap's targets for the enumeration paths, which have no `Result` to put a failure
/// in. Empty on refusal — [`list_capture_sources`] carries the reason instead, so an
/// empty picker is never left unexplained.
#[cfg(not(target_os = "windows"))]
fn scap_targets() -> Vec<scap::Target> {
    access::targets().unwrap_or_default()
}

/// Monitors + windows in one round-trip (the picker queries both together).
///
/// When capture isn't available the lists come back empty **with a reason**: an empty
/// picker and a refused permission look identical to the user otherwise, and the
/// frontend would fall back to showing placeholder devices that don't exist.
pub fn list_capture_sources() -> CaptureSources {
    match access::ensure_permission() {
        Ok(()) => CaptureSources {
            monitors: list_monitors(),
            windows: list_windows(),
            unavailable: None,
        },
        Err(reason) => CaptureSources { monitors: Vec::new(), windows: Vec::new(), unavailable: Some(reason) },
    }
}

/// Microphone inputs via cpal. The `id` is the cpal device name (what the record
/// path resolves back to). System audio is not a device here — it's a separate
/// toggle captured via WASAPI render loopback (see `super::sysaudio`).
pub fn list_audio_inputs() -> Vec<AudioInput> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let mut out = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            if let Ok(name) = d.name() {
                let is_default = default_name.as_deref() == Some(name.as_str());
                out.push(AudioInput { id: name.clone(), name, is_default });
            }
        }
    }
    out
}
