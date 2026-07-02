//! The frontend command surface.
//!
//! [`invoke_handlers!`] expands to `tauri::generate_handler![…]` with every
//! `#[tauri::command]` Arbor exposes. It lives in its own macro (rather than
//! inline in `run()`) so the flat registration list stays out of the entry
//! point — the macro expands at the call site in `lib.rs`, so `generate_handler!`
//! still sees concrete paths.
//!
//! Most git/domain commands route through the single generic `rpc` entry point
//! (`commands::rpc_commands::rpc`) to the Model-D backends; the handlers listed
//! explicitly here are the ones that stay in the shell (window lifecycles,
//! OS-glue, focus/boot handshake, merula, …).

#[macro_export]
macro_rules! invoke_handlers {
    () => {
        tauri::generate_handler![
            // Generic Model-D IPC entry point — the FE forwards every product
            // command here as (program, method, params); the shell router
            // dispatches to the right backend. See `crate::ipc`.
            $crate::commands::rpc_commands::rpc,
            // Appearance preferences (window control style, font scale, …)
            $crate::commands::config_commands::set_explorer_config,
            // Tyto (screen recorder) preferences — opt-in OS-global shortcut
            $crate::commands::config_commands::set_tyto_config,
            // Launcher (Canopy) preferences — per-product window close behaviour
            $crate::commands::config_commands::get_launcher_config,
            $crate::commands::config_commands::set_launcher_close_to_tray,
            // Profile management (keep-shell): CRUD + switch (relaunch).
            $crate::commands::profile_commands::list_profiles,
            $crate::commands::profile_commands::get_active_profile,
            $crate::commands::profile_commands::create_profile,
            $crate::commands::profile_commands::clone_profile,
            $crate::commands::profile_commands::rename_profile,
            $crate::commands::profile_commands::delete_profile,
            $crate::commands::profile_commands::switch_profile,
            // Terminal
            $crate::commands::terminal_commands::terminal_create,
            // Jobs
            $crate::commands::job_commands::cancel_job,
            // App focus / active-tab state (used by focus-gated schedulers) +
            // boot state/handshake (BootSplash recovery).
            $crate::commands::plugin_commands::set_app_focus,
            $crate::commands::plugin_commands::set_active_tab,
            $crate::commands::plugin_commands::get_boot_state,
            $crate::commands::plugin_commands::frontend_ready,
            // Open in browser (OS opener glue)
            $crate::commands::remote_commands::open_in_browser,
            // Filesystem browser
            $crate::commands::fs_commands::fs_set_wallpaper,
            $crate::commands::fs_commands::fs_open_default,
            $crate::commands::fs_commands::fs_reveal_in_dir,
            $crate::commands::fs_commands::fs_open_terminal,
            $crate::commands::fs_commands::fs_show_properties,
            $crate::commands::fs_commands::fs_icon,
            $crate::commands::fs_commands::fs_watch_start,
            $crate::commands::fs_commands::fs_watch_stop,
            $crate::commands::fs_commands::fs_watch_file_start,
            $crate::commands::fs_commands::fs_watch_file_stop,
            // File Explorer "Open in Arbor" delegation (window focus + emit)
            $crate::commands::fs_git_commands::fs_open_in_arbor,
            // Deep-link router (arbor:// URLs)
            $crate::commands::deep_link_commands::deep_link_ready,
            $crate::commands::deep_link_commands::dispatch_deep_link,
            // Marketplace scheduler-rearm interval setters
            $crate::commands::marketplace_commands::marketplace_set_refresh_hours,
            $crate::commands::marketplace_commands::marketplace_set_poll_minutes,
            // ── Window lifecycles ────────────────────────────────────────────
            // Anti-white-flash reveal: every launcher/product window builds hidden
            // and the shell calls this once painted (see `window::window_ready`).
            $crate::window::window_ready,
            // Dedicated File Explorer window + cross-window clipboard & drag
            $crate::window::explorer::open_explorer_window,
            $crate::window::explorer::reveal_in_explorer,
            $crate::window::explorer::take_explorer_reveal,
            $crate::window::explorer::explorer_clip_set,
            $crate::window::explorer::explorer_clip_get,
            $crate::window::explorer::explorer_clip_clear,
            $crate::window::explorer::get_drag_overlay_text,
            $crate::window::explorer::ensure_drag_overlay,
            $crate::window::explorer::drag_overlay_show,
            $crate::window::explorer::drag_overlay_move,
            $crate::window::explorer::drag_overlay_hide,
            $crate::window::explorer::explorer_drop_dispatch,
            // Dedicated merula (music live-coding) window
            $crate::window::merula::open_merula_window,
            // Dedicated Tyto (screen recorder) window
            $crate::window::tyto::open_tyto_window,
            $crate::window::tyto::set_tyto_compact,
            $crate::window::tyto::set_tyto_mini_menu,
            // Tyto frozen-frame region-selection overlay
            $crate::window::region::open_region_selector_window,
            $crate::window::region::get_region_init,
            $crate::window::region::region_selector_confirm,
            $crate::window::region::region_selector_cancel,
            $crate::window::region::region_selector_pick,
            $crate::window::region::take_region_result,
            // Tyto recording HUD (shown while recording, Tyto hidden)
            $crate::window::hud::open_recording_hud,
            $crate::window::hud::close_recording_hud,
            $crate::window::hud::resize_recording_hud,
            $crate::window::hud::get_hud_init,
            // Tyto pre-recording countdown overlay (3-2-1 before video capture)
            $crate::window::countdown::open_countdown_overlay,
            $crate::window::countdown::get_countdown_init,
            $crate::window::countdown::countdown_finished,
            $crate::window::countdown::take_countdown_done,
            $crate::window::countdown::close_countdown_overlay,
            // Dedicated Corvus (git) window
            $crate::window::corvus::open_corvus_window,
            // Launcher window (JetBrains-Toolbox-like home screen)
            $crate::window::launcher::open_launcher_window,
            // Launcher ↔ product-window lifecycle (running-state + Stop)
            $crate::window::list_running_products,
            $crate::window::close_product_window,
            // Full app relaunch (fatal "git backend stopped" overlay recovery)
            $crate::window::restart_app,
            // ── merula engine ─────────────────────────────────────────────────
            // The merula command surface lives out-of-process in `merula-be`
            // (Model-D): the FE reaches it through the generic `rpc` entry point
            // above (program `"merula"`, see `crate::ipc` + `src/lib/ipc/
            // merula.ts`), so NONE of the `merula_*` commands are invoke-routed
            // here. The in-process `crate::merula::*` bodies have been deleted; only
            // `open_merula_window` (window lifecycle, NOT a merula command) stays
            // registered above.
        ]
    };
}
