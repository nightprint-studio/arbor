//! Arbor — Tauri shell entry point.
//!
//! Today `src-tauri` is the **broker + UI container** for the products
//! (Corvus/Git, merula/Music, the File Explorer, …) and is moving toward a
//! JetBrains-Toolbox-like **launcher** (see `window/launcher.rs`). The heavy
//! lifting lives in submodules:
//!
//! - [`app_state`] — the process-wide [`AppState`] managed by Tauri.
//! - [`setup`] — Tauri builder construction + first-run wiring.
//! - [`handlers`] — the `invoke_handlers!` command-registration macro.
//! - [`window`] — the native window lifecycles.
//!
//! [`run`] just stitches those together.

mod app_ctx;
mod app_state;
mod error;
mod handlers;
// merula is fully out-of-process (state + audio substrate in `merula-core`, served
// by the `merula-be` child). The shell keeps only the facade-free launcher-boot
// legacy-storage migration.
mod merula_boot;
mod process_ext;
mod platform;
mod efficiency;
mod taskbar_icon_refresh;
mod git;
mod ide;
mod commands;
mod auth;
mod plugin;
mod config;
mod profile;
mod terminal;
mod jobs;
mod plugin_assets;
mod plugin_host_commands;
mod plugin_logs;
// The effects a wasm extension's host calls turn into. `arbor-plugin-wasm` owns the rules —
// which package may reach what, and in which order — and none of the reaching; this is the
// half that touches the keychain and the network.
mod plugin_wasm;
// Routing the cloud's five storage primitives through a wasm provider when one is installed,
// and falling through to the in-process implementation when none is.
mod cloud_guest;
mod ext;
mod pipeline;
mod git_provider;
mod provider_connect;
mod branding;
mod deep_link;
mod studio;
mod cloud;
mod marketplace;
mod ipc;
// The AI tool surface (MCP): the loopback endpoint, the permission model that gates it,
// and the log of what it did. The protocol itself is `arbor-mcp`; this is the half that
// knows what Arbor is and what the user allowed.
mod mcp;
mod native_menu;
mod setup;
mod window;

pub use app_state::AppState;

// ---------------------------------------------------------------------------
// Tauri entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    setup::init_tracing();
    setup::build_builder()
        .invoke_handler(crate::invoke_handlers!())
        .run(tauri::generate_context!())
        .expect("error while running arbor");
}
