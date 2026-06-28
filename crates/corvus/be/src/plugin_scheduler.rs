//! Plugin-Manager **per-plugin scheduler** ops over the OOP boundary — a
//! sibling of [`crate::plugin_lifecycle`] in the mutation surface that
//! complements the read-only [`crate::plugin_introspect`].
//!
//! After the Phase-2 flip the shell loads NO Corvus product plugins; the live
//! host (and the `Scheduler` wired into it via `install_scheduler` +
//! `start_all_schedulers` in `main.rs`'s `on_ready`) is owned by `main` here in
//! `corvus-be`. The Plugin Manager's "start"/"stop" toggle for a plugin's
//! scheduled action used to route to the shell host and now no-ops for Corvus
//! plugins; this module re-serves it on the host that actually owns the running
//! schedules.
//!
//! ## Accessor — single source of truth
//!
//! There is exactly ONE module-static `Arc<Mutex<PluginHost>>` for corvus-be:
//! the one `plugin_introspect::install(...)` is handed at boot. These handlers
//! borrow that same handle through the `pub(crate)` accessor
//! [`crate::plugin_introspect::host`] (added in the integrate pass), exactly as
//! `plugin_lifecycle` does — no second `install`/static.
//!
//! ## Faithfulness contract
//!
//! Each handler mirrors its shell counterpart in
//! `src-tauri/src/ipc/platform/plugin.rs` (`start_plugin_scheduler` /
//! `stop_plugin_scheduler`) exactly: same host methods
//! (`PluginHost::start_plugin_scheduler` / `::stop_plugin_scheduler`, defined in
//! `crates/plugin/core/src/runtime/scheduler/mod.rs`), same params (`name`,
//! `action`), no `arbor://*` emit (the shell handlers emit nothing — the
//! scheduler engine drives its own runs), and the same error strings. The error
//! mapping is `PluginCoreError::to_string()`, byte-identical to the shell's
//! `AppError` Display for the `Other` variant these methods return (`Other` →
//! `"{s}"`, e.g. `"plugin '<name>' not found"` /
//! `"plugin '<name>' is disabled — enable it first"`), so the wire string the FE
//! sees is unchanged.

use arbor_plugin_core::prelude::PluginHost;
use corvus_core::prelude::CorvusState;

/// Lock the shared plugin host mutably, mapping a poisoned/absent lock onto the
/// same error-string shape `plugin_introspect::with_host` uses for reads. The
/// closure mutates the host; absence means `main` never wired the host (a boot
/// bug, surfaced verbatim).
///
/// Write counterpart of `plugin_introspect`'s read `with_host`: it borrows the
/// SAME module-static handle (via `plugin_introspect::host()`), so there is one
/// host, one lock discipline, no second `install`.
fn with_host_mut<R>(
    f: impl FnOnce(&mut PluginHost) -> Result<R, String>,
) -> Result<R, String> {
    let host = crate::plugin_introspect::host();
    let mut guard = host
        .lock()
        .map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&mut guard)
}

// ---------------------------------------------------------------------------
// Per-plugin scheduler control — start/stop a single scheduled action.
//
// `start_plugin_scheduler` validates the plugin exists + is enabled, then
// registers its `(name, action)` schedule against the shared engine
// (re-registration cancels the old one). `stop_plugin_scheduler` cancels the
// `(name, action)` key if a scheduler is wired. Both are `&mut` host
// pass-throughs — no state file, no hooks, no emit (the engine owns runs).
// ---------------------------------------------------------------------------

/// Start a specific scheduler action for a plugin. Mirrors the shell's
/// `platform::plugin::start_plugin_scheduler`.
#[arbor_rpc::handler]
fn start_plugin_scheduler(
    _ctx: &CorvusState,
    name: String,
    action: String,
) -> Result<(), String> {
    with_host_mut(|host| host.start_plugin_scheduler(&name, &action).map_err(|e| e.to_string()))
}

/// Stop a specific scheduler action for a plugin. Mirrors the shell's
/// `platform::plugin::stop_plugin_scheduler`.
#[arbor_rpc::handler]
fn stop_plugin_scheduler(
    _ctx: &CorvusState,
    name: String,
    action: String,
) -> Result<(), String> {
    with_host_mut(|host| host.stop_plugin_scheduler(&name, &action).map_err(|e| e.to_string()))
}
