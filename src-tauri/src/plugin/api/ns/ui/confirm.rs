//! `arbor.ui.confirm(message, config)`.
//!
//! config: { confirm_label?, confirm_variant?, confirm_action,
//!           cancel_action?, state? }

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (msg, config): (String, Option<mlua::Table>)| {
        let config_json: serde_json::Value = config
            .map(|c| lua_ctx.from_value(mlua::Value::Table(c)).unwrap_or_default())
            .unwrap_or_default();
        let payload = serde_json::json!({
            "plugin_name":     pname,
            "message":         msg,
            "confirm_label":   config_json.get("confirm_label").cloned().unwrap_or(serde_json::json!("Confirm")),
            "confirm_variant": config_json.get("confirm_variant").cloned().unwrap_or(serde_json::json!("primary")),
            "confirm_action":  config_json.get("confirm_action").cloned().unwrap_or(serde_json::json!("")),
            "cancel_action":   config_json.get("cancel_action").cloned(),
            "state":           config_json.get("state").cloned(),
        });
        if let Some(ref h) = handle { let _ = h.emit("plugin:confirm", payload); }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("confirm", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
