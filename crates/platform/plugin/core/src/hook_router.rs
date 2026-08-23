//! Lua-side hook routing: the [`LuaHookListener`] adapter + the low-level
//! dispatch helpers that walk a [`PluginHost`]'s plugins and invoke their
//! `arbor.events.on(...)` handlers.
//!
//! This is the mlua half of the runtime-agnostic hook pipeline. The host fires
//! a hook through an [`arbor_plugin_api::prelude::HookDispatcher`]; the
//! dispatcher fans out to each registered [`HookListener`]; the
//! `LuaHookListener` here translates that into per-plugin Lua calls.
//!
//! Handlers are stored under `__arbor_hooks__` in each plugin's Lua VM, keyed
//! by subscription pattern. Pattern matching is glob-based — `*` matches any
//! substring including across separators.

use std::sync::{Mutex, Weak};

use async_trait::async_trait;
use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_api::prelude::{HookListener, PluginValue};
use arbor_plugin_types::prelude::hook_names;

use crate::error::{PluginCoreError, Result};
use crate::runtime::host::PluginHost;

/// Glob match between a subscription pattern and a concrete event name.
///
/// `*` matches any sequence of characters (including empty / across ':' or '.').
/// Literal strings without `*` must match exactly. This keeps the matcher
/// predictable and cheap — no regex, no segment boundaries.
pub fn matches_pattern(pattern: &str, event: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == event;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut cursor: usize = 0;

    // Anchor the first segment at the start unless the pattern begins with '*'.
    if !parts[0].is_empty() {
        if !event.starts_with(parts[0]) { return false; }
        cursor = parts[0].len();
    }

    // Each intermediate segment must appear somewhere after the cursor.
    if parts.len() >= 3 {
        for seg in &parts[1..parts.len() - 1] {
            if seg.is_empty() { continue; }
            match event[cursor..].find(seg) {
                Some(i) => cursor += i + seg.len(),
                None => return false,
            }
        }
    }

    // Anchor the last segment at the end unless the pattern ends with '*'.
    let last = parts[parts.len() - 1];
    if !last.is_empty() {
        if event.len() < cursor + last.len() { return false; }
        return event[cursor..].ends_with(last);
    }
    true
}

/// Decode the JSON context string into a native Lua table so handlers receive
/// `ctx.field` instead of a JSON string. Falls back to an empty table on any
/// parse / conversion error.
fn ctx_to_lua(lua: &Lua, context_json: &str) -> Result<mlua::Value> {
    let v = match serde_json::from_str::<serde_json::Value>(context_json) {
        Ok(v) => lua.to_value(&v).unwrap_or_else(|_| {
            mlua::Value::Table(
                lua.create_table().expect("failed to create fallback table"),
            )
        }),
        Err(_) => mlua::Value::Table(
            lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?,
        ),
    };
    Ok(v)
}

/// Collect the handler lists in `__arbor_hooks__` whose subscription pattern
/// matches `hook`. Collected up front so a handler that calls
/// `arbor.events.on` while running doesn't mutate the registry mid-iteration.
fn matching_handlers(lua: &Lua, hook: &str) -> Result<Vec<Table>> {
    let registry: Table = lua
        .globals()
        .get("__arbor_hooks__")
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let mut matched: Vec<Table> = Vec::new();
    for pair in registry.pairs::<mlua::Value, Table>() {
        let (key, handlers) = match pair {
            Ok(kv) => kv,
            Err(_) => continue,
        };
        let pattern = match key {
            mlua::Value::String(s) => s.to_str().map(|s| s.to_string()).unwrap_or_default(),
            _ => continue,
        };
        if pattern.is_empty() { continue; }
        if matches_pattern(&pattern, hook) {
            matched.push(handlers);
        }
    }
    Ok(matched)
}

/// Fire a named hook in the given Lua state.
///
/// `context_json` is deserialised to a Lua table and passed as the first
/// argument to every registered handler. Handlers that return an error are
/// logged but do not stop subsequent handlers from running.
pub fn fire(lua: &Lua, hook: &str, context_json: &str) -> Result<()> {
    let ctx = ctx_to_lua(lua, context_json)?;
    for handlers in matching_handlers(lua, hook)? {
        for pair in handlers.sequence_values::<mlua::Function>() {
            let func = match pair {
                Ok(f) => f,
                Err(_) => continue,
            };
            if let Err(e) = func.call::<mlua::Value>(ctx.clone()) {
                crate::lua_ctx::report(lua, "error", format!("hook '{hook}' handler error: {e}"));
            }
        }
    }
    Ok(())
}

/// Fire a hook the same way as [`fire`], but capture every handler's return
/// value into the supplied collector.
///
/// Used by the vetoable path (e.g. `corvus:pre_commit`) where the host needs to
/// know whether any handler asked to abort. Handler errors are logged like in
/// [`fire`] and treated as a non-veto (refuse to block on a buggy plugin).
pub fn fire_collecting(
    lua:          &Lua,
    hook:         &str,
    context_json: &str,
    out:          &mut Vec<mlua::Value>,
) -> Result<()> {
    let ctx = ctx_to_lua(lua, context_json)?;
    for handlers in matching_handlers(lua, hook)? {
        for pair in handlers.sequence_values::<mlua::Function>() {
            let func = match pair { Ok(f) => f, Err(_) => continue };
            match func.call::<mlua::Value>(ctx.clone()) {
                Ok(v)  => out.push(v),
                Err(e) => {
                    crate::lua_ctx::report(
                        lua, "error",
                        format!("hook '{hook}' handler error: {e}"),
                    );
                }
            }
        }
    }
    Ok(())
}

/// Broadcast a hook to every enabled plugin that subscribes to it.
///
/// `arbor:repo_open` / `arbor:tab_switch` additionally refresh the shared
/// `__arbor_current_repo__` global on EVERY loaded plugin (not just the
/// subscribers) so a plugin that never subscribed to repo lifecycle still
/// sees an up-to-date active repo when one of its commands later runs.
pub fn fire_broadcast(host: &PluginHost, hook: &str, context_json: &str) {
    let new_repo_path: Option<String> =
        if hook == hook_names::arbor::REPO_OPEN || hook == hook_names::arbor::TAB_SWITCH {
            serde_json::from_str::<serde_json::Value>(context_json).ok()
                .and_then(|v| v.get("path").and_then(|p| p.as_str()).map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
        } else { None };

    for plugin in &host.plugins {
        if !plugin.is_enabled() { continue; }

        if let Some(ref path) = new_repo_path {
            let _ = plugin.lua.globals().set("__arbor_current_repo__", path.as_str());
        }

        // Plugins with at least one wildcard subscription bypass the manifest
        // filter — they've opted in to seeing any event fired.
        let has_wildcard: bool = plugin.lua.globals()
            .get("__arbor_has_wildcard_hook__")
            .unwrap_or(false);
        if !has_wildcard && !plugin.manifest.hooks.subscribes_to(hook) { continue; }

        if let Err(e) = fire(&plugin.lua, hook, context_json) {
            crate::lua_ctx::report(
                &plugin.lua, "error",
                format!("hook '{hook}' could not be dispatched: {e}"),
            );
        }
    }
}

/// Deliver a hook to a single named plugin only.
///
/// Used for targeted callbacks (job/HTTP/timer results, scheduler-fired Lua
/// actions, per-plugin pipeline requests) where the payload is meant for the
/// one plugin that requested it, not a broadcast. No-op when the plugin is
/// missing or disabled.
pub fn fire_on(host: &PluginHost, plugin_name: &str, hook: &str, context_json: &str) {
    if let Some(plugin) = host.plugins.iter().find(|p| p.manifest.name == plugin_name) {
        if !plugin.is_enabled() { return; }
        if let Err(e) = fire(&plugin.lua, hook, context_json) {
            crate::lua_ctx::report(
                &plugin.lua, "error",
                format!("hook '{hook}' could not be dispatched: {e}"),
            );
        }
    }
}

/// Fire a vetoable hook (e.g. `corvus:pre_commit`) on every subscribing plugin in
/// order, short-circuiting at the first plugin that asks to abort.
///
/// Veto convention:
///   · returning a non-empty string → blocks with that reason
///   · returning `false`            → blocks with empty reason
///   · everything else (nil / true / table / number) → no veto
///
/// Returns the formatted `"<plugin>: <reason>"` (or `"<plugin>: blocked"` for
/// an empty reason) of the first vetoing plugin, or `None` if all let the
/// action proceed. A handler that errors is logged and treated as a non-veto.
pub fn fire_vetoable(host: &PluginHost, hook: &str, context_json: &str) -> Option<String> {
    for plugin in &host.plugins {
        if !plugin.is_enabled() { continue; }
        let has_wildcard: bool = plugin.lua.globals()
            .get("__arbor_has_wildcard_hook__")
            .unwrap_or(false);
        if !has_wildcard && !plugin.manifest.hooks.subscribes_to(hook) { continue; }

        let mut returns: Vec<mlua::Value> = Vec::new();
        if let Err(e) = fire_collecting(&plugin.lua, hook, context_json, &mut returns) {
            crate::lua_ctx::report(
                &plugin.lua, "error",
                format!("vetoable hook '{hook}' could not be dispatched: {e}"),
            );
            continue;
        }

        for v in returns {
            match v {
                mlua::Value::String(s) => {
                    if let Ok(text) = s.to_str() {
                        let reason = text.to_string();
                        return Some(if reason.is_empty() {
                            format!("{}: blocked", plugin.manifest.name)
                        } else {
                            format!("{}: {reason}", plugin.manifest.name)
                        });
                    }
                }
                mlua::Value::Boolean(false) => {
                    return Some(format!("{}: blocked", plugin.manifest.name));
                }
                _ => {}
            }
        }
    }
    None
}

/// The mlua adapter plugged into the runtime-agnostic
/// [`arbor_plugin_api::prelude::HookDispatcher`].
///
/// Holds a weak reference to the host so a host swap / shutdown doesn't keep
/// the dispatcher's listener pinning the `PluginHost` alive. Every fire
/// upgrades the weak; a dropped host turns the delivery into a no-op.
pub struct LuaHookListener {
    host: Weak<Mutex<PluginHost>>,
}

impl LuaHookListener {
    pub fn new(host: Weak<Mutex<PluginHost>>) -> Self {
        Self { host }
    }
}

#[async_trait]
impl HookListener for LuaHookListener {
    async fn fire(&self, name: &str, ctx: &PluginValue) {
        let Some(arc) = self.host.upgrade() else { return; };
        let json = ctx.to_json().to_string();
        // Bind the match so the `MutexGuard` temporary is dropped here rather
        // than living to the end of the async-trait-desugared body.
        match arc.lock() {
            Ok(host) => fire_broadcast(&host, name, &json),
            Err(e)   => tracing::warn!(
                "plugin_host mutex poisoned in LuaHookListener::fire('{name}'): {e}"
            ),
        };
    }

    async fn fire_vetoable(&self, name: &str, ctx: &PluginValue) -> Option<String> {
        let arc = self.host.upgrade()?;
        let json = ctx.to_json().to_string();
        // Bind the result so the `MutexGuard` temporary is dropped before the
        // end of the async-trait-desugared body.
        let veto = match arc.lock() {
            Ok(host) => fire_vetoable(&host, name, &json),
            Err(e)   => {
                tracing::warn!(
                    "plugin_host mutex poisoned in LuaHookListener::fire_vetoable('{name}'): {e}"
                );
                None
            }
        };
        veto
    }
}
