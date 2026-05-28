//! Hook-subscription queries on [`PluginHost`]: `remove_hook` (teardown of a
//! one-off subscription) and `plugin_has_handler` (does a plugin subscribe?).
//!
//! Actual hook *firing* lives in [`crate::hook_router`] — both the broadcast /
//! targeted / vetoable free functions and the [`LuaHookListener`] adapter that
//! the runtime-agnostic `HookDispatcher` drives.
//!
//! [`LuaHookListener`]: crate::hook_router::LuaHookListener

use super::PluginHost;

impl PluginHost {
    /// Drop the entire `__arbor_hooks__[hook]` entry for the given plugin —
    /// callers that built a one-off hook (BRP watch sub, async result hook,
    /// …) use this on teardown so the closure can be freed instead of
    /// hanging around until plugin unload.
    ///
    /// Returns `true` when the key existed and was removed.
    pub fn remove_hook(&self, plugin_name: &str, hook: &str) -> bool {
        let Some(plugin) = self.plugins.iter().find(|p| p.manifest.name == plugin_name) else {
            return false;
        };
        let registry: mlua::Table = match plugin.lua.globals().get("__arbor_hooks__") {
            Ok(t) => t,
            Err(_) => return false,
        };
        // Set the key to nil to drop the entry. `set(_, Nil)` is the canonical
        // remove form in mlua — `raw_remove` only exists on sequences.
        registry.set(hook, mlua::Value::Nil).is_ok()
    }

    /// Whether `plugin_name` has at least one live handler subscribed for
    /// `hook` (literal name OR glob pattern in `__arbor_hooks__`).
    ///
    /// Used by routed Tauri commands (e.g. `request_pipeline_run`) to decide
    /// whether to delegate to the plugin or fall back to a built-in default.
    /// Returns `false` when the plugin is missing, disabled, or has no
    /// matching handler — never errors.
    pub fn plugin_has_handler(&self, plugin_name: &str, hook: &str) -> bool {
        let plugin = match self.plugins.iter().find(|p| p.manifest.name == plugin_name) {
            Some(p) => p,
            None => return false,
        };
        if !plugin.is_enabled() { return false; }

        let registry: mlua::Table = match plugin.lua.globals().get("__arbor_hooks__") {
            Ok(t) => t,
            Err(_) => return false,
        };

        for pair in registry.pairs::<mlua::Value, mlua::Table>() {
            let (key, handlers) = match pair { Ok(kv) => kv, Err(_) => continue };
            let pattern = match key {
                mlua::Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
                _ => continue,
            };
            if pattern.is_empty() { continue; }
            if !crate::hook_router::matches_pattern(&pattern, hook) { continue; }
            // Match — confirm the handler list isn't empty (a plugin may have
            // called `arbor.events.off` and left a key with zero handlers).
            if handlers.raw_len() > 0 { return true; }
        }
        false
    }
}
