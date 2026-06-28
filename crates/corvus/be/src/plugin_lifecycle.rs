//! Plugin-Manager **write/runtime** ops over the OOP boundary — the mutation
//! twin of [`crate::plugin_introspect`].
//!
//! After the Phase-2 flip the shell loads NO Corvus product plugins; the live
//! host is owned by `main` here in `corvus-be`. `plugin_introspect` already
//! re-served the **read/reflection** subset (list, dep-graph, settings, cascade
//! previews) as `corvus`-program RPC handlers. This module re-serves the
//! **mutation** subset so the Plugin Manager's enable/disable actions operate on
//! the host that actually owns the plugins instead of no-op'ing against the
//! empty shell host.
//!
//! ## Accessor — single source of truth
//!
//! There is exactly ONE module-static `Arc<Mutex<PluginHost>>` for corvus-be:
//! the one `plugin_introspect::install(...)` is handed at boot. Rather than add a
//! second `install`/static here, the write handlers borrow that same handle
//! through the `pub(crate)` accessor [`crate::plugin_introspect::host`] (added in
//! the same integrate pass). Read handlers take `&PluginHost`; write handlers
//! lock it `&mut` and mutate.
//!
//! ## Faithfulness contract
//!
//! Each handler mirrors its shell counterpart in
//! `src-tauri/src/ipc/platform/plugin.rs` byte-for-byte: same host methods, same
//! `plugin_states.json` writes (done *inside* the host methods — `enable_plugin`
//! / `disable_plugin` persist the new enable-state themselves via
//! `save_plugin_states`), same `on_plugin_load` / `on_plugin_unload` hooks (fired
//! *inside* `enable_one_plugin` / `disable_one_plugin`), and the same error
//! strings. The error mapping is `PluginCoreError::to_string()`, which is
//! byte-identical to the shell's `AppError` Display for the `Plugin`/`Io`/`Other`
//! variants these methods return (`Other` → `"{s}"`, `Plugin` → `"Plugin error:
//! {s}"`), so the wire string the FE sees is unchanged.
//!
//! ### A note on `arbor://plugins-reloaded`
//!
//! The shell's `enable_plugin` / `disable_plugin` handlers do **not** emit
//! `arbor://plugins-reloaded` — they return the cascade list and the FE refetches
//! `list_plugin_info` off that. We replicate that exactly: no emit here. (The
//! reload / master-toggle ops, which *do* emit, are the Phase-2 agents' surface.)

use arbor_plugin_core::prelude::PluginHost;
use corvus_core::prelude::CorvusState;

/// Lock the shared plugin host mutably, mapping a poisoned/absent lock onto the
/// same error-string shape `plugin_introspect::with_host` uses for reads. The
/// closure mutates the host; absence means `main` never wired the host (a boot
/// bug, surfaced verbatim).
///
/// This is the **write** counterpart of `plugin_introspect`'s read `with_host`:
/// it borrows the SAME module-static handle (via `plugin_introspect::host()`),
/// so there is one host, one lock discipline, no second `install`.
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
// Enable / disable — with the transitive dependency cascade the host does.
//
// `enable_plugin` enables `name` + every transitively-required dep that's off
// (deps-first, target last); refuses with a blocker summary when a required dep
// is missing/unloadable (call `plugin_enable_preview` first). `disable_plugin`
// disables `name` + every transitively-enabled dependent (leaves-first). Both
// persist `plugin_states.json` and fire on_plugin_load/unload *inside* the host
// methods — see `crates/plugin/core/src/runtime/host/lifecycle.rs`. The returned
// `Vec<String>` is the ordered list of names actually toggled, which the FE uses
// to refresh the affected rows.
// ---------------------------------------------------------------------------

/// Enable a plugin (transitive required deps + target, deps-first). Errors when
/// a required dep is missing/unloadable — call `plugin_enable_preview` first.
/// Mirrors the shell's `platform::plugin::enable_plugin`.
#[arbor_rpc::handler]
fn enable_plugin(_ctx: &CorvusState, name: String) -> Result<Vec<String>, String> {
    with_host_mut(|host| host.enable_plugin(&name).map_err(|e| e.to_string()))
}

/// Disable a plugin + every transitively-required dependent (leaves-first).
/// Mirrors the shell's `platform::plugin::disable_plugin`.
#[arbor_rpc::handler]
fn disable_plugin(_ctx: &CorvusState, name: String) -> Result<Vec<String>, String> {
    with_host_mut(|host| host.disable_plugin(&name).map_err(|e| e.to_string()))
}
