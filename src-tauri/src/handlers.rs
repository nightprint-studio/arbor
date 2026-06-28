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
            // Dedicated merula (music live-coding) window
            $crate::window::merula::open_merula_window,
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
            // eval / transport / render / sample packs / config
            $crate::merula::merula_eval,
            $crate::merula::merula_transport,
            $crate::merula::merula_render,
            $crate::merula::merula_render_stems,
            $crate::merula::merula_export_midi,
            $crate::merula::merula_analyze_levels,
            $crate::merula::merula_packs,
            $crate::merula::merula_pack_download,
            $crate::merula::merula_pack_reindex,
            $crate::merula::merula_pack_delete,
            $crate::merula::get_merula_config,
            $crate::merula::set_merula_config,
            $crate::merula::merula_audio_devices,
            $crate::merula::merula_set_output_device,
            // Fase 4: arrangement query / sound bank / live mixer / window
            // state / project model (all additive)
            $crate::merula::query::merula_query,
            $crate::merula::scenes::merula_scenes,
            $crate::merula::scenes::merula_launch,
            $crate::merula::sounds::merula_sounds,
            $crate::merula::merula_set_track,
            $crate::merula::merula_audition_expr,
            $crate::merula::merula_eval_snippet,
            $crate::merula::merula_materialize,
            $crate::merula::merula_play_snippet,
            $crate::merula::merula_stop_snippet,
            $crate::merula::state::get_merula_state,
            $crate::merula::state::set_merula_state,
            $crate::merula::state::get_merula_project_tabs,
            $crate::merula::state::set_merula_project_tabs,
            $crate::merula::state::get_merula_project_mix,
            $crate::merula::state::set_merula_project_mix,
            $crate::merula::state::get_merula_aliases,
            $crate::merula::state::set_merula_aliases,
            $crate::merula::state::get_merula_scratch_tabs,
            $crate::merula::state::set_merula_scratch_tabs,
            $crate::merula::project::merula_open_project,
            $crate::merula::project::merula_create_project,
            $crate::merula::project::merula_set_project_name,
            $crate::merula::reference::merula_lang_reference,
            $crate::merula::format::merula_format,
            $crate::merula::scales::merula_scales,
            $crate::merula::libraries::merula_libraries,
            $crate::merula::libraries::merula_sync_libraries,
            // merula import: WAV → MIDI (transcription) / MIDI → .merula
            $crate::merula::import::merula_convert_wav_to_midi,
            $crate::merula::import::merula_import_audio_as_merula,
            $crate::merula::import::merula_import_midi_as_merula,
            // merula ONNX transcription models (download on-demand)
            $crate::merula::models::merula_models,
            $crate::merula::models::merula_download_model,
            $crate::merula::models::merula_delete_model,
        ]
    };
}
