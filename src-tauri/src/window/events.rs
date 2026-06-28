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
            let label = window.label();
            // Product windows (Corvus/Merula/Sitta) honour the launcher's
            // close-to-tray setting: when ON, closing the window just HIDES it —
            // the product stays running (still lit in the launcher) and is
            // terminated only via the launcher's Stop, which force-destroys and
            // bypasses this path. When OFF (default), the window closes for real
            // (→ `Destroyed` → emit `running:false` + teardown). The launcher
            // staying in control is what guarantees no un-killable zombie.
            if let Some(id) = super::product_id_for_label(label) {
                let keep = crate::config::app_config::load()
                    .ok()
                    .and_then(|c| c.launcher.products.get(id).map(|p| p.close_to_tray))
                    .unwrap_or(false);
                if keep {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
            }
            #[cfg(not(debug_assertions))]
            {
                // Close-to-tray for the main (launcher) window in release.
                // Auxiliary windows (the drag-ghost overlay, …) close for real.
                if label == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(debug_assertions)]
            let _ = api;
        }
        WindowEvent::Destroyed => {
            let label = window.label();
            // nemus audio teardown happens on ACTUAL destroy — not on a
            // close-to-tray hide (where the window lives on and may still play).
            if label == super::nemus::NEMUS_WINDOW_LABEL {
                crate::nemus::shutdown(window.app_handle());
            }
            // When the last window of a product is gone, tell the launcher its
            // node is no longer "In esecuzione". Count the OTHER windows of the
            // same product (this one is being torn down) regardless of whether
            // it's already left the manager's map.
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
