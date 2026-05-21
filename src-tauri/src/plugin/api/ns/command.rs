//! `arbor.command` — command palette registration.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::dual_write_contribution;
use crate::plugin::contribution::points;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let cmd_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_register(ctx, lua, &cmd_table)?;
    install_unregister(ctx, lua, &cmd_table)?;

    arbor.set("command", cmd_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register(ctx: &ApiCtx, lua: &Lua, cmd_table: &Table) -> Result<()> {
    // register({ id, title, description?, icon?, group? })
    // — sugar for arbor.ui.contribute("arbor:command-palette", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
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
        let payload = serde_json::json!({
            "title":       title,
            "description": description,
            "icon":        icon,
            "group":       group,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::COMMAND_PALETTE, &id, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    cmd_table.set("register", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_unregister(ctx: &ApiCtx, lua: &Lua, cmd_table: &Table) -> Result<()> {
    let pname    = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, id: String| {
        if contribs.remove(&pname, points::COMMAND_PALETTE, &id) {
            contribs.notify_changed(&handle, points::COMMAND_PALETTE);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    cmd_table.set("unregister", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
