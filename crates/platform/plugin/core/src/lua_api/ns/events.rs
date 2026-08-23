//! `arbor.events.on` / `arbor.events.emit` — unified subscribe / emit.
//!
//!   arbor.events.on(event, fn)        -- subscribe to a built-in hook
//!                                        (e.g. "corvus:commit") OR to a
//!                                        plugin event (e.g.
//!                                        "compile-action:build_done").
//!                                        '*' wildcards are supported in the
//!                                        event string.
//!
//!   arbor.events.emit(event, payload) -- emit a custom event. If the name
//!                                        contains no ':' it is auto-prefixed
//!                                        with the calling plugin's name.
//!                                        If a prefix is provided it MUST
//!                                        match the caller's plugin name —
//!                                        publishing under another plugin's
//!                                        namespace is rejected.
//!
//! Built-in hooks are emitted by the host (commit, push, repo_open, …) and
//! travel on the same `__arbor_hooks__` plumbing, so subscribers don't have
//! to distinguish "hook" from "custom event".
//!
//! ## The optional prefix on subscribe (**D9**)
//!
//! `emit` has always auto-prefixed an unqualified name with the caller's plugin
//! name. `on` now applies the mirror rule with the *host product*'s id: inside
//! `garrulus-be`, `arbor.events.on("note_saved", …)` can only mean
//! `garrulus:note_saved`. Already-qualified names and wildcard patterns are
//! taken exactly as written, so `on("corvus:commit", …)` and `on("garrulus:*", …)`
//! keep working from any host. See
//! [`resolve_subscription`](arbor_plugin_types::prelude::resolve_subscription)
//! for the full rule, including why lifecycle names fall back to the `arbor:`
//! host namespace instead of taking the product prefix.
//!
//! ## Why subscribe-time validation exists
//!
//! Lua has no compile step, so a mistyped hook name used to be invisible in
//! both directions: nothing fires `garrulus:note_savd`, so the handler simply
//! never runs and the plugin presents as "does nothing" with not one message
//! anywhere. The subscription is still registered — a plugin subscribing to a
//! hook a future Arbor will add must not fail to load — but it is announced in
//! the Plugin Logs panel with the nearest catalog entries.
//!
//! Delivery is asynchronous: emit() spawns a background thread that calls
//! `hook_router::fire_broadcast` so we never deadlock when emitting from
//! inside a hook handler.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_types::prelude::{hook_catalog, hook_ns};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

/// How many catalog entries a "did you mean" line suggests. Three is enough to
/// cover the near-miss and the same event in a neighbouring namespace without
/// turning the log line into a wall.
const MAX_SUGGESTIONS: usize = 3;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let events_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    install_on(ctx, lua, &events_table)?;
    install_emit(ctx, lua, &events_table)?;

    arbor.set("events", events_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_on(ctx: &ApiCtx, lua: &Lua, events_table: &Table) -> Result<()> {
    // Snapshot at install time: the closure outlives the ApiCtx.
    let product = ctx.product.clone();

    let fn_ = lua.create_function(move |lua_ctx, (event, func): (String, mlua::Function)| {
        // A host that never bound a product (headless / unit-test runs) cannot
        // resolve anything, so the name is taken verbatim rather than guessed.
        let resolved = match product.as_deref() {
            Some(product) => hook_catalog::resolve_subscription(&event, product).into_owned(),
            None => event.clone(),
        };

        warn_if_unknown(lua_ctx, &event, &resolved);

        let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
        let list: mlua::Result<Table> = registry.get(resolved.clone());
        let list = match list {
            Ok(t)  => t,
            Err(_) => {
                let t = lua_ctx.create_table()?;
                registry.set(resolved.clone(), t.clone())?;
                t
            }
        };
        list.push(func)?;
        if hook_ns::is_pattern(&resolved) {
            lua_ctx.globals().set("__arbor_has_wildcard_hook__", true)?;
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    events_table.set("on", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Announce a subscription that nothing will ever deliver.
///
/// Only fires for names that were *meant* as built-ins. Two signals say so:
/// the plugin wrote the name unqualified (there is no other way to reach an
/// unqualified name — `emit` always publishes under a plugin namespace), or it
/// wrote a namespace the host actually fires in. A subscription to another
/// plugin's event (`compile-action:build_done`) matches neither and stays
/// silent, which is the whole reason the check is not simply "not in catalog".
fn warn_if_unknown(lua: &Lua, requested: &str, resolved: &str) {
    // A pattern is a subscription to a shape, not to a name: `garrulus:*` is
    // legal, and so is a pattern that only matches hooks a later release adds.
    if hook_ns::is_pattern(resolved) || hook_catalog::find(resolved).is_some() {
        return;
    }

    let was_unqualified = hook_ns::split_ns(requested).is_none();
    let host_namespace = hook_ns::namespace_of(resolved)
        .is_some_and(hook_catalog::is_known_namespace);
    if !was_unqualified && !host_namespace {
        return;
    }

    let suggestions = hook_catalog::nearest(resolved, MAX_SUGGESTIONS);
    let did_you_mean = if suggestions.is_empty() {
        String::new()
    } else {
        format!(" Did you mean {}?", suggestions.join(", "))
    };
    // Report both what was written and what it resolved to: with an optional
    // prefix in play, "note_savd" and "garrulus:note_savd" are different halves
    // of the same mistake and each on its own reads as a typo in the message.
    let resolved_as = if requested == resolved {
        String::new()
    } else {
        format!(" (resolved from '{requested}')")
    };
    let message = format!(
        "arbor.events.on: '{resolved}'{resolved_as} is not a hook Arbor fires.{did_you_mean} \
         The subscription is registered anyway, but nothing will deliver to it."
    );

    crate::lua_ctx::report(lua, "warn", message);
}

fn install_emit(ctx: &ApiCtx, lua: &Lua, events_table: &Table) -> Result<()> {
    let host  = ctx.host_weak.clone();
    let pname = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (event, payload): (String, Option<mlua::Value>)| {
        // Resolve full event name.
        let full_event = match event.find(':') {
            None => format!("{}:{}", pname, event),
            Some(_) => {
                let prefix = event.split(':').next().unwrap_or("");
                if prefix != pname {
                    return Err(mlua::Error::RuntimeError(format!(
                        "arbor.events.emit: plugin '{pname}' cannot publish to namespace \
                         '{prefix}' (event '{event}') — drop the prefix or use your own \
                         plugin name"
                    )));
                }
                event.clone()
            }
        };

        // Serialise payload to JSON once so every subscribing Lua VM
        // receives an equivalent table (hook_router decodes it).
        let ctx_json = match payload {
            None | Some(mlua::Value::Nil) => "{}".to_string(),
            Some(v) => {
                let json: serde_json::Value = lua_ctx
                    .from_value(v)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                serde_json::to_string(&json)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
            }
        };

        if let Some(weak) = host.clone() {
            std::thread::spawn(move || {
                if let Some(arc) = weak.upgrade() {
                    if let Ok(host) = arc.lock() {
                        crate::hook_router::fire_broadcast(&host, &full_event, &ctx_json);
                    }
                }
            });
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    events_table.set("emit", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
