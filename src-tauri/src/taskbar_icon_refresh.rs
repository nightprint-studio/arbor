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
//! Second failure mode handled here: when `explorer.exe` (the shell) crashes
//! and restarts, it re-creates the taskbar and broadcasts the registered
//! `"TaskbarCreated"` window message. Every app that owns a taskbar button
//! must re-apply its icon on that message or the button shows up blank until
//! the next explicit `set_icon`. We subclass the `main` window's `WndProc`,
//! chaining to tao/WebView2's original proc, and re-run the same icon refresh
//! whenever that message arrives.
//!
//! Linux / macOS: no-op.

#[cfg(target_os = "windows")]
mod imp {
    use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
    use std::sync::OnceLock;
    use tauri::{AppHandle, Manager};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::Power::{
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, RegisterWindowMessageW, SetWindowLongPtrW, GWLP_WNDPROC, WNDPROC,
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
            // Reconcile OS power-throttling: the suspend/resume transition can
            // reset process priorities (and spawn fresh WebView2 renderers),
            // and no fresh focus event is guaranteed on wake. Force the
            // efficiency worker to re-apply the current desired state off the
            // UI thread so we don't come back stuck at IDLE priority.
            crate::efficiency::force_reapply();
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

    // --- "TaskbarCreated" WndProc subclass ---------------------------------
    //
    // The atom returned by RegisterWindowMessageW("TaskbarCreated"); 0 means
    // "not registered yet / failed". The previous window proc is stashed as an
    // isize (a WNDPROC fn-pointer) so our_proc can chain to it.
    static TASKBAR_CREATED_MSG: AtomicU32 = AtomicU32::new(0);
    static PREV_WNDPROC: AtomicIsize = AtomicIsize::new(0);

    unsafe extern "system" fn our_wndproc(
        hwnd:   HWND,
        msg:    u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let taskbar_created = TASKBAR_CREATED_MSG.load(Ordering::Relaxed);
        if taskbar_created != 0 && msg == taskbar_created {
            // Explorer restarted → re-apply the taskbar icon. refresh_taskbar_icon
            // dispatches the actual set_icon to the main thread, so this stays
            // non-blocking inside the WndProc.
            refresh_taskbar_icon();
            tracing::info!(
                target: "arbor::focus",
                "[taskbar_icon] TaskbarCreated received — re-applying icon"
            );
        }
        // Always chain to tao/WebView2's original proc so normal message
        // handling is preserved.
        let prev = PREV_WNDPROC.load(Ordering::Relaxed);
        let prev_proc: WNDPROC = std::mem::transmute::<isize, WNDPROC>(prev);
        CallWindowProcW(prev_proc, hwnd, msg, wparam, lparam)
    }

    fn install_taskbar_created_hook(app: &AppHandle) {
        let Some(win) = app.get_webview_window("main") else {
            tracing::warn!("[taskbar_icon] no `main` window for TaskbarCreated hook");
            return;
        };
        let hwnd = match win.hwnd() {
            Ok(h) => h.0 as HWND,
            Err(e) => {
                tracing::warn!("[taskbar_icon] hwnd() failed: {e}");
                return;
            }
        };
        unsafe {
            // Register the shell's broadcast message id (idempotent per string).
            let name: [u16; 15] = [
                b'T' as u16, b'a' as u16, b's' as u16, b'k' as u16, b'b' as u16,
                b'a' as u16, b'r' as u16, b'C' as u16, b'r' as u16, b'e' as u16,
                b'a' as u16, b't' as u16, b'e' as u16, b'd' as u16, 0,
            ];
            let msg_id = RegisterWindowMessageW(name.as_ptr());
            if msg_id == 0 {
                tracing::warn!("[taskbar_icon] RegisterWindowMessageW(TaskbarCreated) failed");
                return;
            }
            TASKBAR_CREATED_MSG.store(msg_id, Ordering::Relaxed);

            // Swap in our proc, stashing the previous one to chain to. If a
            // previous proc was already stored we've installed once — bail.
            if PREV_WNDPROC.load(Ordering::Relaxed) != 0 {
                return;
            }
            let our = our_wndproc as usize as isize;
            let prev = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, our);
            if prev == 0 {
                tracing::warn!("[taskbar_icon] SetWindowLongPtrW(GWLP_WNDPROC) failed");
                return;
            }
            PREV_WNDPROC.store(prev, Ordering::Relaxed);
            tracing::info!("[taskbar_icon] subclassed main WndProc for TaskbarCreated");
        }
    }

    pub fn install(app: &AppHandle) {
        // Publish the handle first so the TaskbarCreated proc (and the power
        // callback) can read it. Both refresh paths route through APP_HANDLE.
        let first_install = APP_HANDLE.set(app.clone()).is_ok();

        // The TaskbarCreated subclass must run even in debug (where the tray
        // isn't installed) — a blank taskbar button is user-visible regardless.
        // Guarded against double-install internally via PREV_WNDPROC.
        install_taskbar_created_hook(app);

        if !first_install {
            return; // power notification already registered
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
