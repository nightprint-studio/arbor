//! Real enumeration of capture targets. Backs the `sources` domain handlers,
//! replacing the frontend mock fixtures. Per-item errors are swallowed (skip the
//! bad monitor/window/device) so one failure never empties the whole picker.
//!
//! On Windows the rich metadata (monitor resolution/primary, window title + app name)
//! comes from `windows-capture` (the crate scap uses under the hood) plus a small
//! Win32 process-name helper, joined to scap's capture side by the shared u32 handle
//! id. On other platforms it falls back to scap's own (thinner) enumeration.

use cpal::traits::{DeviceTrait, HostTrait};

use crate::sources::{AudioInput, CaptureSources, MonitorSource, WindowSource};

use super::access;

/// Monitors as wire structs (`mon-<id>` ids the FE hands back on select).
pub fn list_monitors() -> Vec<MonitorSource> {
    #[cfg(target_os = "windows")]
    {
        win_monitors()
    }
    #[cfg(not(target_os = "windows"))]
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
    #[cfg(not(target_os = "windows"))]
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
        let w = m.width().unwrap_or(0);
        let h = m.height().unwrap_or(0);
        let primary = Some(m.as_raw_hmonitor() as usize) == primary;
        out.push(MonitorSource {
            id: format!("mon-{id}"),
            name,
            resolution: format!("{w} × {h}"),
            // windows-capture doesn't expose the DPI scale; the FE only displays it.
            scale: 1.0,
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

/// Physical top-left origin + DPI scale of a monitor (`mon-<id>`). Region selection
/// needs both to map a CSS-pixel drag onto the captured display. Defaults to
/// `(0, 0, 1.0)` off Windows or on lookup failure.
pub fn monitor_geometry(monitor_id: &str) -> (i32, i32, f64) {
    #[cfg(target_os = "windows")]
    {
        win_monitor_geometry(monitor_id)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = monitor_id;
        (0, 0, 1.0)
    }
}

#[cfg(target_os = "windows")]
fn win_monitor_geometry(monitor_id: &str) -> (i32, i32, f64) {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

    let id = match monitor_id.strip_prefix("mon-").and_then(|s| s.parse::<u32>().ok()) {
        Some(id) => id,
        None => return (0, 0, 1.0),
    };
    unsafe {
        let hmon = HMONITOR(id as usize as *mut std::ffi::c_void);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            rcMonitor: RECT::default(),
            rcWork: RECT::default(),
            dwFlags: 0,
        };
        if !GetMonitorInfoW(hmon, &mut info).as_bool() {
            return (0, 0, 1.0);
        }
        let (x, y) = (info.rcMonitor.left, info.rcMonitor.top);
        let mut dpi_x = 96u32;
        let mut dpi_y = 96u32;
        let scale = if GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok() {
            dpi_x as f64 / 96.0
        } else {
            1.0
        };
        (x, y, scale)
    }
}

// ── Non-Windows: scap's own (thinner) enumeration ────────────────────────────

#[cfg(not(target_os = "windows"))]
fn scap_monitors() -> Vec<MonitorSource> {
    scap_targets()
        .into_iter()
        .filter_map(|t| match t {
            scap::Target::Display(d) => Some(MonitorSource {
                id: format!("mon-{}", d.id),
                name: d.title,
                resolution: String::new(),
                scale: 1.0,
                primary: false,
            }),
            _ => None,
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
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
