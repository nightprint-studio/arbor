//! `arbor.events.on` / `arbor.events.emit` — unified subscribe / emit.
//!
//!   arbor.events.on(event, fn)        -- subscribe to a built-in hook
//!                                        (e.g. "on_repo_open") OR to a
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
//! Delivery is asynchronous: emit() spawns a background thread that calls
//! `PluginHost.fire_hook` so we never deadlock when emitting from inside a
//! hook handler.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let events_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_on(lua, &events_table)?;
    install_emit(ctx, lua, &events_table)?;

    arbor.set("events", events_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_on(lua: &Lua, events_table: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, (event, func): (String, mlua::Function)| {
        let registry: Table = lua_ctx.globals().get("__arbor_hooks__")?;
        let list: mlua::Result<Table> = registry.get(event.clone());
        let list = match list {
            Ok(t)  => t,
            Err(_) => {
                let t = lua_ctx.create_table()?;
                registry.set(event.clone(), t.clone())?;
                t
            }
        };
        list.push(func)?;
        if event.contains('*') {
            lua_ctx.globals().set("__arbor_has_wildcard_hook__", true)?;
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    events_table.set("on", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_emit(ctx: &ApiCtx, lua: &Lua, events_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
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
        // receives an equivalent table (hook_registry decodes it).
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

        if let Some(ref h) = handle {
            let h2 = h.clone();
            std::thread::spawn(move || {
                let state = h2.state::<crate::AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    let _ = host.fire_hook(&full_event, &ctx_json);
                };
            });
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    events_table.set("emit", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
