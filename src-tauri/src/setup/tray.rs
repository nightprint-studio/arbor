//! System-tray icon + menu. Release builds only (the dev runner's
//! rebuild/relaunch cycle would leak tray icons).
//!
//! On macOS the very same `TrayIconBuilder` produces a **menu-bar extra**
//! (`NSStatusItem`), which is why the ambient surfaces live here rather than in
//! per-platform code: one menu, rendered natively on each OS.
//!
//! The menu is the entry point for Arbor's [`Ambient`](crate::window::SurfaceKind::Ambient)
//! surfaces — the ones that must be reachable *while you are working in another
//! application*. Tyto (the recorder) is the case in point: hunting for its
//! window in Alt-Tab defeats the purpose of a screen recorder, so capture starts
//! and stops from here (or from its global shortcut) without surfacing Tyto at
//! all.
//!
//! Dev builds have no tray, so these entries are release-only; in dev the same
//! actions stay reachable from the launcher, the Command Palette and the Tyto
//! global shortcut.

#[cfg(not(debug_assertions))]
pub fn install(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::Manager;

    let show = MenuItem::with_id(app, "show", "Show Arbor", true, None::<&str>)?;
    let explorer = MenuItem::with_id(app, "explorer", "Open File Explorer", true, None::<&str>)?;
    // ── Tyto (ambient) ──
    // "Stop recording" stays enabled at all times: a menu item whose enabled
    // state has to be pushed from another thread is a portability hazard, and
    // the action is already a no-op when nothing is recording. Pressing it
    // without a recording simply does nothing.
    let tyto_snip = MenuItem::with_id(app, "tyto_snip", "Snip capture", true, None::<&str>)?;
    let tyto_stop = MenuItem::with_id(app, "tyto_stop", "Stop recording", true, None::<&str>)?;
    let tyto_open = MenuItem::with_id(app, "tyto_open", "Open Tyto", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &explorer,
            &sep2,
            &tyto_snip,
            &tyto_stop,
            &tyto_open,
            &sep,
            &quit,
        ],
    )?;

    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .tooltip("Arbor")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "explorer" => crate::window::explorer::open_or_focus(app),
            // Straight into region selection — the quick-capture path, same as
            // the global shortcut.
            "tyto_snip" => crate::window::tyto::open_or_focus_snip(app),
            "tyto_stop" => crate::window::tyto::request_stop_recording(app),
            "tyto_open" => crate::window::tyto::open_or_focus(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    if w.is_visible().unwrap_or(false) {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
