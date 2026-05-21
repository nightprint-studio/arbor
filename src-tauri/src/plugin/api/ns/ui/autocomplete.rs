//! `arbor.ui.set_autocomplete_options(id, options)`.
//!
//! Push a fresh list of suggestions to an open autocomplete field.
//! The field identifies itself by `id` (set in the form node). Options
//! may be bare strings (auto-expanded to {value,label}) or full
//! `{ value, label, group? }` tables.

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (id, opts_table): (String, mlua::Table)| {
        let mut options: Vec<serde_json::Value> = Vec::new();
        for pair in opts_table.sequence_values::<mlua::Value>() {
            let val = match pair { Ok(v) => v, Err(_) => continue };
            match val {
                mlua::Value::String(s) => {
                    let v = s.to_str().map(|x| x.to_string()).unwrap_or_default();
                    options.push(serde_json::json!({ "value": v, "label": v }));
                }
                mlua::Value::Table(row) => {
                    let value = row.get::<String>("value").unwrap_or_default();
                    let label = row.get::<String>("label").unwrap_or_else(|_| value.clone());
                    let group = row.get::<Option<String>>("group").unwrap_or(None);
                    options.push(serde_json::json!({ "value": value, "label": label, "group": group }));
                }
                _ => { /* skip */ }
            }
            // Silence unused-var warnings from lua_ctx when bare strings
            // are the only branch exercised at runtime.
            let _ = lua_ctx;
        }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:autocomplete-options", serde_json::json!({
                "plugin_name": pname, "id": id, "options": options,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("set_autocomplete_options", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
