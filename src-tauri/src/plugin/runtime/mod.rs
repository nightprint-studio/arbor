//! Plugin runtime — manifest parsing, host registry, lifecycle, hooks,
//! services, pipeline ops, scheduler engine.
//!
//! The module is split across small files mirroring the conceptual layers:
//!
//!   * `consts`    — API contract version, app version, host OS string
//!   * `manifest/` — `plugin.toml` shape (permissions, hooks, schedule, deps,
//!                   info) plus discovery + topological sort + persisted
//!                   enabled-state file
//!   * `loaded`    — `LoadedPlugin` / `DormantPlugin` + per-plugin cancel maps
//!   * `host/`     — `PluginHost` registry, lifecycle, hooks, services,
//!                   pipeline-op invocation, frontend-facing introspection
//!   * `scheduler/`— OS-thread scheduler engine (fixed_rate / fixed_delay /
//!                   cron) with focus-gated firing
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

// ── Manifest types ───────────────────────────────────────────────────────────
pub use manifest::{
    PluginManifest, discover_plugins, plugin_dir,
};
pub use manifest::deps::{PluginDependency, PluginLoadFailure};
pub use manifest::info::{ComboOption, PluginHooks, PluginInfo, PluginSandbox};
pub use manifest::permissions::{
    AccessLevel, EnvReadPerm, GitLevel, PluginPermissions, TerminalLevel,
};
pub use manifest::schedule::{
    PluginSchedule, PluginScheduleStatus, PluginSchedulerSection,
    ScheduleRegistry, ScheduleTrigger, parse_duration_secs,
};

// ── Loaded-plugin state ──────────────────────────────────────────────────────
pub use loaded::{DormantPlugin, LoadedPlugin, TimerCancels, TimerCounter};

// ── Host (registry, lifecycle, hooks, service, pipeline-op, introspection) ──
pub use host::PluginHost;
pub use host::lifecycle::load_plugin;
pub use host::pipeline_op::PipelineOpResult;
pub use host::service::ServiceError;
