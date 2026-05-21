//! `fire_hook` / `fire_hook_on` — broadcast or direct hook delivery.

use crate::error::Result;

use super::PluginHost;

impl PluginHost {
    pub fn fire_hook(&self, hook: &str, context_json: &str) -> Result<()> {
        // `__arbor_current_repo__` is shared state that EVERY plugin reads via
        // `arbor.repo.current()` / `arbor.settings.project.*`. Refresh it for
        // every loaded plugin BEFORE the per-plugin subscription gate, so a
        // plugin that doesn't care to subscribe to repo lifecycle hooks (e.g.
        // a one-shot palette command) still sees an up-to-date active repo
        // when invoked. Without this, only plugins listed under `[hooks]
        // on_repo_open|on_tab_switch` would have their global refreshed.
        let new_repo_path: Option<String> =
            if hook == "on_repo_open" || hook == "on_tab_switch" {
                serde_json::from_str::<serde_json::Value>(context_json).ok()
                    .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()))
                    .filter(|s| !s.is_empty())
            } else { None };

        for plugin in &self.plugins {
            if !plugin.is_enabled() { continue; }

            if let Some(ref path) = new_repo_path {
                let _ = plugin.lua.globals().set("__arbor_current_repo__", path.as_str());
            }

            // Plugins with at least one wildcard subscription bypass the
            // manifest filter — they've opted in to seeing any event fired.
            let has_wildcard: bool = plugin.lua.globals()
                .get("__arbor_has_wildcard_hook__")
                .unwrap_or(false);
            if !has_wildcard && !plugin.manifest.hooks.subscribes_to(hook) { continue; }

            if let Err(e) = crate::plugin::hook_registry::fire(&plugin.lua, hook, context_json) {
                tracing::warn!("hook '{hook}' error in '{}': {e}", plugin.manifest.name);
            }
        }
        Ok(())
    }

    pub fn fire_hook_on(&self, plugin_name: &str, hook: &str, context_json: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.iter().find(|p| p.manifest.name == plugin_name) {
            if !plugin.is_enabled() { return Ok(()); }
            if let Err(e) = crate::plugin::hook_registry::fire(&plugin.lua, hook, context_json) {
                tracing::warn!("hook '{hook}' error in '{plugin_name}': {e}");
            }
        }
        Ok(())
    }

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
            if !crate::plugin::hook_registry::matches_pattern(&pattern, hook) { continue; }
            // Match — confirm the handler list isn't empty (a plugin may have
            // called `arbor.events.off` and left a key with zero handlers).
            if handlers.raw_len() > 0 { return true; }
        }
        false
    }

    /// One veto entry collected from a vetoable hook (e.g. `on_pre_commit`).
    /// `reason` is whatever the plugin returned from its handler; an empty
    /// string means the plugin blocked without a stated reason.
    pub fn collect_veto(
        &self,
        hook:         &str,
        context_json: &str,
    ) -> Vec<(String /* plugin */, String /* reason */)> {
        let mut out = Vec::new();
        for plugin in &self.plugins {
            if !plugin.is_enabled() { continue; }
            let has_wildcard: bool = plugin.lua.globals()
                .get("__arbor_has_wildcard_hook__")
                .unwrap_or(false);
            if !has_wildcard && !plugin.manifest.hooks.subscribes_to(hook) { continue; }

            let mut returns: Vec<mlua::Value> = Vec::new();
            if let Err(e) = crate::plugin::hook_registry::fire_collecting(
                &plugin.lua, hook, context_json, &mut returns,
            ) {
                tracing::warn!("hook '{hook}' fire_collecting error in '{}': {e}", plugin.manifest.name);
                continue;
            }

            // Veto convention:
            //   · returning a string  → blocks with that reason
            //   · returning `false`   → blocks with empty reason
            //   · everything else (nil / true / table / number) → no veto
            // Strings are the easiest path for plugin authors and round-trip
            // cleanly through Lua's missing-return semantics (a plain
            // `return` yields nil → no block, `return "msg"` yields a veto).
            for v in returns {
                match v {
                    mlua::Value::String(s) => {
                        if let Ok(text) = s.to_str() {
                            out.push((plugin.manifest.name.clone(), text.to_string()));
                        }
                    }
                    mlua::Value::Boolean(false) => {
                        out.push((plugin.manifest.name.clone(), String::new()));
                    }
                    _ => {}
                }
            }
        }
        out
    }
}
