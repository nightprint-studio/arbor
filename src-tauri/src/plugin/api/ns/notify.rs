//! `arbor.notify(cfg)` — in-app notification center (top-level function).
//!
//!   arbor.notify{
//!     message = "exit 0 in 12s",       -- required, non-empty string
//!     title   = "Build done",          -- optional, defaults to ""
//!     level   = "success",             -- optional: "info"|"success"|"warning"|"error"
//!     toast   = true,                  -- optional, default true
//!                                      --   when false: only added to the
//!                                      --   bell, no transient toast
//!     persist = true,                  -- optional, default true
//!                                      --   when false: only the transient
//!                                      --   toast fires, the bell is left
//!                                      --   alone (use this for "started"
//!                                      --   chatter you don't need to read
//!                                      --   later)
//!     action  = {                      -- optional: click-action table
//!       kind   = "open-link-manager",
//!       label  = "View link",
//!       link_id = "...",
//!     },
//!   }
//!
//! Boundary validation: malformed input raises a Lua error (programming
//! error — not a recoverable failure). The supported `action.kind` shapes
//! are documented alongside the notifications overlay.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Value| {
        let cfg_table = match cfg {
            mlua::Value::Table(t) => t,
            _ => return Err(mlua::Error::RuntimeError(
                "arbor.notify: expected a config table { message, title?, level?, action? }".to_string()
            )),
        };

        let title: String = cfg_table.get::<Option<String>>("title")
            .ok().flatten()
            .unwrap_or_default();

        let message: String = cfg_table.get("message").map_err(|_| mlua::Error::RuntimeError(
            "arbor.notify: 'message' must be a string".to_string()
        ))?;
        if message.is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.notify: 'message' must be a non-empty string".to_string()
            ));
        }

        let level: String = cfg_table.get::<Option<String>>("level")
            .ok().flatten()
            .unwrap_or_else(|| "info".to_string());
        match level.as_str() {
            "info" | "success" | "warning" | "error" => {}
            other => return Err(mlua::Error::RuntimeError(format!(
                "arbor.notify: 'level' must be one of info|success|warning|error (got '{other}')"
            ))),
        }

        let action_json = match cfg_table.get::<mlua::Value>("action").ok() {
            Some(mlua::Value::Nil) | None => None,
            Some(v) => Some(lua_ctx.from_value::<serde_json::Value>(v).map_err(|e|
                mlua::Error::RuntimeError(format!("arbor.notify: invalid 'action': {e}"))
            )?),
        };

        // Channel selectors. Both default to true so existing call sites that
        // don't pass these fields keep their original "toast + bell" behavior.
        let toast: bool   = cfg_table.get::<Option<bool>>("toast").ok().flatten().unwrap_or(true);
        let persist: bool = cfg_table.get::<Option<bool>>("persist").ok().flatten().unwrap_or(true);

        if let Some(ref h) = handle {
            let mut payload = serde_json::json!({
                "plugin":  pname,
                "title":   title,
                "message": message,
                "level":   level,
                "toast":   toast,
                "persist": persist,
            });
            if let Some(act) = action_json {
                payload["action"] = act;
            }
            let _ = h.emit("plugin:notification", payload);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    arbor.set("notify", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
