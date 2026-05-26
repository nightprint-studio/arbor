//! Re-applies the main window's icon after the system resumes from
//! sleep / hibernation. Works around a Windows + WebView2 bug where the
//! taskbar's small `HICON` for the app goes blank after S3/Modern Standby
//! resume (Alt+Tab still shows the correct icon because it uses the
//! window-class `ICON_BIG`, while the taskbar caches `ICON_SMALL` and loses
//! it during the power transition).
//!
//! Implementation: `PowerRegisterSuspendResumeNotification` with a callback,
//! which fires `PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND` on wake.
//! On either, we dispatch back to the main thread and call
//! [`tauri::WebviewWindow::set_icon`] with `app.default_window_icon()`.
//!
//! Linux / macOS: no-op.

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::OnceLock;
    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY,
    };

    // PBT_* event-type codes for the power-notify callback. Mirrors winuser.h
    // — these are stable Win32 constants going back to XP/Vista.
    const PBT_APMRESUMESUSPEND:   u32 = 0x07;
    const PBT_APMRESUMEAUTOMATIC: u32 = 0x12;

    // DEVICE_NOTIFY_CALLBACK flag for PowerRegisterSuspendResumeNotification
    // — declared inline because windows-sys 0.52 doesn't re-export it.
    const DEVICE_NOTIFY_CALLBACK: u32 = 0x2;

    // PowerRegisterSuspendResumeNotification lives in user32.dll since
    // Windows 8 and isn't surfaced by windows-sys 0.52 either (only the
    // Unregister side is). Declared inline against the documented signature
    // — stable since Win8.
    #[link(name = "user32")]
    extern "system" {
        fn PowerRegisterSuspendResumeNotification(
            flags:               u32,
            recipient:           *const core::ffi::c_void,
            registrationhandle:  *mut HPOWERNOTIFY,
        ) -> u32;
    }

    // Held for the lifetime of the process. The callback reads APP_HANDLE
    // to know which window to refresh; REGISTRATION keeps the handle alive
    // so we never get unregistered out from under us.
    static APP_HANDLE:   OnceLock<AppHandle> = OnceLock::new();
    static REGISTRATION: OnceLock<isize>     = OnceLock::new();

    unsafe extern "system" fn power_callback(
        _context:   *const core::ffi::c_void,
        event_type: u32,
        _setting:   *const core::ffi::c_void,
    ) -> u32 {
        if event_type == PBT_APMRESUMEAUTOMATIC || event_type == PBT_APMRESUMESUSPEND {
            refresh_taskbar_icon();
        }
        0 // ERROR_SUCCESS
    }

    fn refresh_taskbar_icon() {
        let Some(app) = APP_HANDLE.get() else { return };
        let icon = match app.default_window_icon() {
            Some(i) => i.clone(),
            None    => return,
        };
        let app_for_run = app.clone();
        let _ = app.run_on_main_thread(move || {
            let Some(win) = app_for_run.get_webview_window("main") else { return };
            match win.set_icon(icon) {
                Ok(_)  => tracing::info!(
                    target: "arbor::focus",
                    "[taskbar_icon] refreshed after power resume"
                ),
                Err(e) => tracing::warn!("[taskbar_icon] set_icon failed: {e}"),
            }
        });
    }

    pub fn install(app: &AppHandle) {
        if APP_HANDLE.set(app.clone()).is_err() {
            return; // already installed
        }
        unsafe {
            // Box::leak so the params struct outlives the registration —
            // PowerRegisterSuspendResumeNotification stores the pointer and
            // dereferences it on each event. We never unregister.
            let params = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(power_callback),
                Context:  std::ptr::null_mut(),
            }));
            let mut handle: HPOWERNOTIFY = 0;
            let result = PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                params as *const _ as *const core::ffi::c_void,
                &mut handle,
            );
            if result == 0 {
                let _ = REGISTRATION.set(handle);
                tracing::info!("[taskbar_icon] registered power-resume notification");
            } else {
                tracing::warn!(
                    "[taskbar_icon] PowerRegisterSuspendResumeNotification failed: {result}"
                );
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    use tauri::AppHandle;
    #[inline(always)]
    pub fn install(_app: &AppHandle) {}
}

/// Install the OS power-resume hook that re-applies the main window icon
/// after the system wakes from sleep. Idempotent — calling more than once
/// is a no-op. Call from Tauri's `setup` callback.
pub fn install(app: &tauri::AppHandle) {
    imp::install(app);
}
