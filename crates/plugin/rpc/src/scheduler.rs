//! Per-plugin scheduler control — start/stop a single scheduled action. Generic
//! port of `corvus-be`'s former `plugin_scheduler.rs`.
//!
//! Both are `&mut` host pass-throughs against the shared trigger engine wired
//! into the host (no state file, no hooks, no emit — the engine owns runs).
//! `start_plugin_scheduler` validates the plugin exists + is enabled, then
//! registers its `(name, action)` schedule (re-registration cancels the old one).
//! `stop_plugin_scheduler` cancels the `(name, action)` key. Error mapping is
//! `PluginCoreError::to_string()` — the same wire string the shell produced.

use crate::context::{with_host_mut, PluginRpcContext};

/// Start a specific scheduler action for a plugin.
pub fn start_plugin_scheduler<C: PluginRpcContext>(
    ctx: &C,
    name: String,
    action: String,
) -> Result<(), String> {
    with_host_mut(ctx, |host| {
        host.start_plugin_scheduler(&name, &action).map_err(|e| e.to_string())
    })
}

/// Stop a specific scheduler action for a plugin.
pub fn stop_plugin_scheduler<C: PluginRpcContext>(
    ctx: &C,
    name: String,
    action: String,
) -> Result<(), String> {
    with_host_mut(ctx, |host| {
        host.stop_plugin_scheduler(&name, &action).map_err(|e| e.to_string())
    })
}
