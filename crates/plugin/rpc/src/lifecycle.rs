//! Plugin enable/disable with the host's transitive dependency cascade. Generic
//! port of `corvus-be`'s former `plugin_lifecycle.rs`.
//!
//! `enable_plugin` enables `name` + every transitively-required dep that's off
//! (deps-first); refuses with a blocker summary when a required dep is
//! missing/unloadable. `disable_plugin` disables `name` + every transitively-
//! enabled dependent (leaves-first). Both persist `plugin_states.json` and fire
//! `on_plugin_load`/`on_plugin_unload` **inside** the host methods, and return
//! the ordered list of names actually toggled. The error mapping is
//! `PluginCoreError::to_string()` — byte-identical to the shell's wire string.

use crate::context::{with_host_mut, PluginRpcContext};

/// Enable a plugin (transitive required deps + target, deps-first). Errors when a
/// required dep is missing/unloadable — call `plugin_enable_preview` first.
pub fn enable_plugin<C: PluginRpcContext>(ctx: &C, name: String) -> Result<Vec<String>, String> {
    with_host_mut(ctx, |host| host.enable_plugin(&name).map_err(|e| e.to_string()))
}

/// Disable a plugin + every transitively-required dependent (leaves-first).
pub fn disable_plugin<C: PluginRpcContext>(ctx: &C, name: String) -> Result<Vec<String>, String> {
    with_host_mut(ctx, |host| host.disable_plugin(&name).map_err(|e| e.to_string()))
}
