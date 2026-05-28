//! `arbor.command` — command palette registration + invocation.
//!
//!   register / unregister  → palette entry (always available, no permission)
//!   fire                    → invoke a registered command (needs `command_invoke`)

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_types::prelude::{AccessLevel, GitLevel, RequiredPerm, TerminalLevel};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;
use crate::lua_api::helpers::contrib_write::dual_write_contribution;
use crate::contribution::points;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let cmd_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    install_register(ctx, lua, &cmd_table)?;
    install_unregister(ctx, lua, &cmd_table)?;
    // `.fire` is gated on the command_invoke permission — a plugin without it
    // simply has no `arbor.command.fire`, mirroring `arbor.service.call`.
    if ctx.command_invoke {
        install_fire(ctx, lua, &cmd_table)?;
    }

    arbor.set("command", cmd_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register(ctx: &ApiCtx, lua: &Lua, cmd_table: &Table) -> Result<()> {
    // register({ id, title, description?, icon?, group?, invocable?, required? })
    // — sugar for arbor.ui.contribute("arbor:command-palette", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.command.register: 'id' is required".to_string())
        })?;
        let title = config.get::<String>("title").map_err(|_| {
            mlua::Error::RuntimeError("arbor.command.register: 'title' is required".to_string())
        })?;
        let description = config.get::<Option<String>>("description").unwrap_or(None);
        let icon        = config.get::<Option<String>>("icon").unwrap_or(None);
        let group       = config.get::<Option<String>>("group").unwrap_or(None);
        let invocable   = config.get::<Option<bool>>("invocable").unwrap_or(None).unwrap_or(false);
        let required    = parse_required(config.get::<Option<mlua::Table>>("required").unwrap_or(None));
        let payload = serde_json::json!({
            "title":       title,
            "description": description,
            "icon":        icon,
            "group":       group,
            "invocable":   invocable,
            "required":    required,
        });
        dual_write_contribution(
            &contribs, &pname,
            points::COMMAND_PALETTE, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    cmd_table.set("register", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_unregister(ctx: &ApiCtx, lua: &Lua, cmd_table: &Table) -> Result<()> {
    let pname    = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let fn_ = lua.create_function(move |_, id: String| {
        if contribs.remove(&pname, points::COMMAND_PALETTE, &id) {
            contribs.notify_changed(points::COMMAND_PALETTE);
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    cmd_table.set("unregister", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_fire(ctx: &ApiCtx, lua: &Lua, cmd_table: &Table) -> Result<()> {
    // fire(id, ctx?) — invoke a registered command. Fire-and-forget: the call
    // is dispatched on a background thread so it never blocks (and can't
    // deadlock on the non-reentrant PluginHost mutex). Failures are logged to
    // the plugin's log stream; there is no return value (commands are
    // fire-and-forget, like the palette flow).
    let host   = ctx.host_weak.clone();
    let caller = ctx.plugin_name.clone();
    let fn_ = lua.create_function(
        move |lua_ctx, (id, ctx_val): (String, Option<mlua::Value>)| {
            let ctx_json: serde_json::Value = match ctx_val {
                None | Some(mlua::Value::Nil) => serde_json::json!({}),
                Some(v) => lua_ctx.from_value(v)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?,
            };
            let caller_p = caller.clone();
            let host_c   = host.clone();
            std::thread::spawn(move || {
                if let Some(arc) = host_c.and_then(|w| w.upgrade()) {
                    if let Ok(host) = arc.lock() {
                        if let Err(e) = host.invoke_command(&caller_p, &id, &ctx_json) {
                            tracing::warn!(
                                target: "plugin",
                                "arbor.command.fire('{id}') in '{caller_p}': {} ({})",
                                e.message(), e.kind(),
                            );
                        }
                    }
                }
            });
            Ok(())
        },
    ).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    cmd_table.set("fire", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Parse the Lua `required` table into a [`RequiredPerm`]. Recognises a single
/// `{ <domain> = "<level>" }` pair; the first known domain wins. Unknown
/// domains / levels / a missing table all collapse to `None`.
fn parse_required(t: Option<mlua::Table>) -> RequiredPerm {
    let Some(t) = t else { return RequiredPerm::None; };
    let level = |key: &str| t.get::<Option<String>>(key).ok().flatten();
    let parse = |s: String| serde_json::Value::String(s);

    if let Some(v) = level("git") {
        if let Ok(l) = serde_json::from_value::<GitLevel>(parse(v)) { return RequiredPerm::Git(l); }
    }
    if let Some(v) = level("fs") {
        if let Ok(l) = serde_json::from_value::<AccessLevel>(parse(v)) { return RequiredPerm::Fs(l); }
    }
    if let Some(v) = level("issues") {
        if let Ok(l) = serde_json::from_value::<AccessLevel>(parse(v)) { return RequiredPerm::Issues(l); }
    }
    if let Some(v) = level("provider") {
        if let Ok(l) = serde_json::from_value::<AccessLevel>(parse(v)) { return RequiredPerm::Provider(l); }
    }
    if let Some(v) = level("toolchain") {
        if let Ok(l) = serde_json::from_value::<AccessLevel>(parse(v)) { return RequiredPerm::Toolchain(l); }
    }
    if let Some(v) = level("terminal") {
        if let Ok(l) = serde_json::from_value::<TerminalLevel>(parse(v)) { return RequiredPerm::Terminal(l); }
    }
    RequiredPerm::None
}
