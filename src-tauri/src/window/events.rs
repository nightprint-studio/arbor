//! Shared `on_window_event` handler for every Arbor window.
//!
//! Three concerns live here, keyed off the window label / event kind:
//!  - the **merula** window closing tears down its audio session;
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
                // Tyto is EXEMPT from close-to-tray: closing it always terminates the
                // recorder. A screen recorder lingering invisibly in the tray is a
                // privacy footgun (the user thinks it's gone), so there is no
                // "keep alive" option for it — the launcher never offers the toggle and
                // this path never honours one even if the config somehow held it.
                if id != "tyto" {
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
            }
            // Close-to-tray for the main (launcher) window in release.
            // Auxiliary windows (the drag-ghost overlay, …) close for real.
            //
            // Windows/Linux only. On macOS a window that vanishes into a tray
            // icon on ⌘W is a foreign gesture: the platform expects the window
            // to close and the *application* to stay alive (it does — the Dock
            // icon and the menu bar are the app), so the user reopens Canopy
            // from the Dock, not from a hidden tray. Hiding instead would also
            // strand the window in the ⌘-Tab list with nothing to click.
            #[cfg(all(not(debug_assertions), not(target_os = "macos")))]
            {
                if label == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            #[cfg(any(debug_assertions, target_os = "macos"))]
            let _ = api;
        }
        WindowEvent::Destroyed => {
            let label = window.label();
            // The window directory shrank: refresh every open switcher and
            // Window menu. Broadcast for ALL windows, not just product ones —
            // the launcher and the explorer list there too.
            super::emit_windows_changed(window.app_handle());
            // merula audio teardown is handled out-of-process: the real session
            // lives in the `merula-be` child, torn down by the `split_broker`
            // detach below on the last merula window's actual destroy (the shell
            // holds no in-process merula state to shut down).
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
                tracing::info!(
                    "window Destroyed: label={label} product={id} still_running={still_running}"
                );
                if !still_running {
                    super::emit_product_state(app, id, false);
                    // Tear down the product's headless backend along with its last
                    // window. We only reach `Destroyed` on a REAL close — a
                    // close-to-tray hide is intercepted in `CloseRequested` and
                    // never gets here — so the backend is safe to kill: the user
                    // actually closed (or the launcher force-stopped) the product.
                    // `detach()` drops the `BrokerClient` Arc, whose `Drop` closes
                    // the stdio pipe and reaps the child. The child's own
                    // disconnect callback re-emits `…-down` to a now-gone window
                    // (harmless); the next open re-spawns a fresh backend.
                    //
                    // Every product with a lazy OOP backend tears down here so the
                    // headless child never lingers windowless (corvus/merula/sitta/tyto/
                    // bennu). Sitta included: without it the explorer's `sitta-be` would
                    // survive its last window and a re-open would silently reuse the
                    // stale process (no respawn, no `…-up` reload) instead of a fresh
                    // one — diverging from corvus/merula. Tyto included so closing the
                    // recorder actually ends `tyto-be` (it never minimizes to tray).
                    // Bennu included so closing the Java editor actually ends `bennu-be`
                    // (otherwise the launcher shows it down while the process lingers).
                    //
                    // Safe to call inline on the UI thread: `detach` removes the
                    // routing entry under a brief lock and offloads the blocking
                    // child `kill()`+`wait()` to its own thread (it used to run that
                    // teardown under the routing lock on this very thread, freezing
                    // the launcher and every other product's IPC mid-close).
                    if matches!(id, "corvus" | "merula" | "sitta" | "tyto" | "bennu") {
                        crate::ipc::split_broker::detach(id, "window-closed");
                    }
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

            // JetBrains-Toolbox-style auto-hide: when the launcher loses focus
            // it slips back to the tray. Launching a product, an Alt-Tab, or a
            // click on any other window all surface here as `Focused(false)`, so
            // the launcher gets out of the way exactly like the Toolbox does —
            // summon it again from the tray icon. Release only: dev builds have
            // no tray, and a stray focus loss (DevTools, editor) must not make
            // the window vanish — the shell shows a dev-only close button
            // instead. Product windows are untouched (only launcher labels).
            //
            // Windows/Linux only, like the close-to-tray path above: on macOS
            // the launcher vanishing the moment you click another window reads
            // as a crash, and there is no tray icon in the user's mental model
            // to summon it back from — Canopy stays put and is dismissed the
            // way every mac window is.
            #[cfg(all(not(debug_assertions), not(target_os = "macos")))]
            if !focused && super::is_launcher_label(window.label()) {
                let _ = window.hide();
            }
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
