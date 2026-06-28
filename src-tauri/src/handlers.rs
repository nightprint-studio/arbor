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
//! OS-glue, focus/boot handshake, nemus, …).

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
            // Launcher (Canopy) preferences — per-product window close behaviour
            $crate::commands::config_commands::get_launcher_config,
            $crate::commands::config_commands::set_launcher_close_to_tray,
            // Profile management (keep-shell): CRUD + switch (relaunch).
            $crate::commands::profile_commands::list_profiles,
            $crate::commands::profile_commands::get_active_profile,
            $crate::commands::profile_commands::create_profile,
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
            // File Explorer "Open in Arbor" delegation (window focus + emit)
            $crate::commands::fs_git_commands::fs_open_in_arbor,
            // Deep-link router (arbor:// URLs)
            $crate::commands::deep_link_commands::deep_link_ready,
            $crate::commands::deep_link_commands::dispatch_deep_link,
            // Marketplace scheduler-rearm interval setters
            $crate::commands::marketplace_commands::marketplace_set_refresh_hours,
            $crate::commands::marketplace_commands::marketplace_set_poll_minutes,
            // ── Window lifecycles ────────────────────────────────────────────
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
            // Dedicated nemus (music live-coding) window
            $crate::window::nemus::open_nemus_window,
            // Dedicated Corvus (git) window
            $crate::window::corvus::open_corvus_window,
            // Launcher window (JetBrains-Toolbox-like home screen)
            $crate::window::launcher::open_launcher_window,
            // Launcher ↔ product-window lifecycle (running-state + Stop)
            $crate::window::list_running_products,
            $crate::window::close_product_window,
            // Full app relaunch (fatal "git backend stopped" overlay recovery)
            $crate::window::restart_app,
            // ── nemus engine ─────────────────────────────────────────────────
            // eval / transport / render / sample packs / config
            $crate::nemus::nemus_eval,
            $crate::nemus::nemus_transport,
            $crate::nemus::nemus_render,
            $crate::nemus::nemus_render_stems,
            $crate::nemus::nemus_export_midi,
            $crate::nemus::nemus_analyze_levels,
            $crate::nemus::nemus_packs,
            $crate::nemus::nemus_pack_download,
            $crate::nemus::nemus_pack_reindex,
            $crate::nemus::nemus_pack_delete,
            $crate::nemus::get_nemus_config,
            $crate::nemus::set_nemus_config,
            $crate::nemus::nemus_audio_devices,
            $crate::nemus::nemus_set_output_device,
            // Fase 4: arrangement query / sound bank / live mixer / window
            // state / project model (all additive)
            $crate::nemus::query::nemus_query,
            $crate::nemus::scenes::nemus_scenes,
            $crate::nemus::scenes::nemus_launch,
            $crate::nemus::sounds::nemus_sounds,
            $crate::nemus::nemus_set_track,
            $crate::nemus::nemus_audition_expr,
            $crate::nemus::nemus_eval_snippet,
            $crate::nemus::nemus_materialize,
            $crate::nemus::nemus_play_snippet,
            $crate::nemus::nemus_stop_snippet,
            $crate::nemus::state::get_nemus_state,
            $crate::nemus::state::set_nemus_state,
            $crate::nemus::state::get_nemus_project_tabs,
            $crate::nemus::state::set_nemus_project_tabs,
            $crate::nemus::state::get_nemus_project_mix,
            $crate::nemus::state::set_nemus_project_mix,
            $crate::nemus::state::get_nemus_aliases,
            $crate::nemus::state::set_nemus_aliases,
            $crate::nemus::state::get_nemus_scratch_tabs,
            $crate::nemus::state::set_nemus_scratch_tabs,
            $crate::nemus::project::nemus_open_project,
            $crate::nemus::project::nemus_create_project,
            $crate::nemus::project::nemus_set_project_name,
            $crate::nemus::reference::nemus_lang_reference,
            $crate::nemus::format::nemus_format,
            $crate::nemus::scales::nemus_scales,
            $crate::nemus::libraries::nemus_libraries,
            $crate::nemus::libraries::nemus_sync_libraries,
            // nemus import: WAV → MIDI (transcription) / MIDI → .nemus
            $crate::nemus::import::nemus_convert_wav_to_midi,
            $crate::nemus::import::nemus_import_audio_as_nemus,
            $crate::nemus::import::nemus_import_midi_as_nemus,
            // nemus ONNX transcription models (download on-demand)
            $crate::nemus::models::nemus_models,
            $crate::nemus::models::nemus_download_model,
            $crate::nemus::models::nemus_delete_model,
        ]
    };
}
