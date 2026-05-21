//! `arbor.ui.pick_file` — opens the native-feeling FilePickerModal.
//!
//! Example (Lua):
//!   arbor.ui.pick_file({
//!     mode        = "file",                     -- "file" | "folder" | "save"
//!     title       = "Select stage JSON",
//!     extensions  = { "json" },                 -- "file"/"save" only
//!     initial_path = "/home/u/Documents/…",
//!     action      = "source-export:on_stage_picked",   -- REQUIRED
//!     extra       = { profile_id = "cfg_xyz" }, -- optional, echoed back
//!   })
//!
//!   arbor.events.on("source-export:on_stage_picked", function(ctx)
//!     -- ctx.path is "" when the user cancelled
//!     if ctx.path == "" then return end
//!     io_ex.import_stage(ctx.profile_id, ctx.path)
//!   end)

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: mlua::Table| {
        let json_val: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(opts))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let mut payload = serde_json::json!({ "plugin_name": pname });
        if let serde_json::Value::Object(ref obj) = json_val {
            for (k, v) in obj { payload[k] = v.clone(); }
        }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:pick-file", payload);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("pick_file", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
