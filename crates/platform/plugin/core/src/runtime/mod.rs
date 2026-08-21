//! Plugin runtime — manifest discovery, host registry, lifecycle, hooks,
//! services, pipeline ops, scheduler engine.
//!
//! The module is split across small files mirroring the conceptual layers:
//!
//!   * `consts`    — API contract version, app version, host OS string
//!   * `manifest/` — `plugin.toml` discovery + topological sort + persisted
//!     enabled-state file. The pure-data manifest shapes (`Manifest`,
//!     `Permissions`, `Hooks`, …) live in the `arbor-plugin-types` crate;
//!     this module re-exports what the rest of the host historically reached
//!     for under `crate::plugin::runtime::manifest::*`.
//!   * `loaded`    — `LoadedPlugin` / `DormantPlugin` + per-plugin cancel maps
//!   * `host/`     — `PluginHost` registry, lifecycle, hooks, services,
//!     pipeline-op invocation, frontend-facing introspection
//!   * `scheduler/`— OS-thread scheduler engine (fixed_rate / fixed_delay /
//!     cron) with focus-gated firing
//!
//! Everything that was previously importable as `crate::plugin::runtime::*`
//! is re-exported flat from this module so existing callers keep compiling.

#![allow(unused_imports)]

pub mod consts;
pub mod host;
pub mod loaded;
pub mod manifest;
pub mod scheduler;

// ── Constants ────────────────────────────────────────────────────────────────
pub use consts::{ARBOR_API_VERSION, ARBOR_APP_VERSION, current_os};

// ── Manifest discovery + state ───────────────────────────────────────────────
pub use manifest::{
    discover_in_roots, discover_plugins, forget_plugin_state, install_plugin_for,
    load_plugin_states, load_plugin_states_for, load_states, plugin_dir, plugin_products,
    save_plugin_states, save_plugin_states_for, save_states, set_plugin_state_for,
    PluginStates,
};
pub use manifest::info::{ComboOption, PluginInfo};

// ── Loaded-plugin state ──────────────────────────────────────────────────────
pub use loaded::{
    DormantPlugin, LoadedPlugin, PluginActivity, ServiceIndex, TimerCancel, TimerCancels,
    TimerCounter,
};

// ── Host (registry, lifecycle, hooks, service, pipeline-op, introspection) ──
pub use host::PluginHost;
pub use host::lifecycle::load_plugin;
pub use host::pipeline_op::PipelineOpResult;
pub use host::service::ServiceError;
pub use host::command::{host_command_required, CommandError};
