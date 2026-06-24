//! Shared `on_window_event` handler for every Arbor window.
//!
//! Three concerns live here, keyed off the window label / event kind:
//!  - the **nemus** window closing tears down its audio session;
//!  - **close-to-tray** for the `main` window in release (auxiliary windows
//!    close for real);
//!  - **efficiency mode** (OS power-throttle) driven by focus + minimize.

use tauri::{Manager, WindowEvent};

use crate::AppState;

/// Route a native window event. Wired via `Builder::on_window_event` in
/// `setup::build_builder`.
pub fn handle(window: &tauri::Window, event: &WindowEvent) {
    match event {
        WindowEvent::CloseRequested { api, .. } => {
            // The nemus window closing for real tears down its audio session
            // (drops the cpal stream on the audio thread, stops sound). Lazy
            // ownership: nothing happens if it never played.
            if window.label() == super::nemus::NEMUS_WINDOW_LABEL {
                crate::nemus::shutdown(window.app_handle());
            }
            #[cfg(not(debug_assertions))]
            {
                // Close-to-tray applies ONLY to the main window. Auxiliary
                // windows (the dedicated File Explorer, the drag-ghost overlay,
                // product windows) close for real — otherwise a closed window is
                // merely hidden and reopening re-summons the same stale window
                // instead of a fresh one.
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(debug_assertions)]
            let _ = api;
        }
        WindowEvent::Destroyed => {
            // When the last window of a product is gone, tell the launcher its
            // node is no longer "In esecuzione". Count the OTHER windows of the
            // same product (this one is being torn down) regardless of whether
            // it's already left the manager's map.
            let label = window.label();
            if let Some(id) = super::product_id_for_label(label) {
                let app = window.app_handle();
                let still_running = app
                    .webview_windows()
                    .keys()
                    .any(|l| l.as_str() != label && super::product_id_for_label(l) == Some(id));
                if !still_running {
                    super::emit_product_state(app, id, false);
                }
            }
        }
        WindowEvent::Focused(focused) => {
            let focused = *focused;
            // Update the app-focused flag so focus-gated schedulers work correctly.
            let state = window.app_handle().state::<AppState>();
            state.app_focused.store(focused, std::sync::atomic::Ordering::Relaxed);
            // Signal the desired OS power-throttle state (EcoQoS on Windows,
            // nice/sched on Linux/macOS). Handled here in the native
            // window-event callback rather than via a frontend IPC call so
            // minimize / Alt-Tab / window-switch are all caught reliably via
            // Win32 WM_SETFOCUS / WM_KILLFOCUS messages. The actual (expensive)
            // process scan runs off-thread in the efficiency worker.
            crate::efficiency::request(!focused);
        }
        WindowEvent::Resized(size) => {
            // Windows reports minimize as a Resized event with width=0, height=0.
            // Focused(false) alone doesn't always fire on minimize (depending on
            // desktop/window-manager behavior), so we trigger efficiency mode
            // from here too as a belt-and-braces catch.
            if size.width == 0 && size.height == 0 {
                let state = window.app_handle().state::<AppState>();
                state.app_focused.store(false, std::sync::atomic::Ordering::Relaxed);
                crate::efficiency::request(true);
            }
        }
        _ => {}
    }
}
