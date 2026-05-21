//! `arbor.contribution` — introspection of the contribution registry.
//!
//! Plugins can read what every other plugin (including themselves) has
//! contributed and which contribution points have been declared. Useful
//! for: knowing whether a host section has been overridden, conditional
//! defaulting, plugin-orchestrating-plugin setups.
//!
//!   arbor.contribution.list(point) → { item, item, … }
//!     Each item: { plugin_name, item_id, payload, priority, when?,
//!                  disabled?, group? }. `payload` is a Lua table.
//!
//!   arbor.contribution.list_points() → { point, point, … }
//!     Each point: { plugin_name, name, description?, schema? }.
//!
//! Reads only. There is no `subscribe`: contribution changes already fire
//! the `arbor://contributions-changed` Tauri event and plugins that need
//! to react can listen via their normal event hooks.

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let contrib_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    {
        let reg = ctx.contributions.clone();
        let fn_ = lua.create_function(move |lua_ctx, point: String| {
            let items = reg.list_for_point(&point);
            let json = serde_json::to_value(&items).unwrap_or(serde_json::Value::Array(Vec::new()));
            Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        contrib_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    {
        let reg = ctx.contributions.clone();
        let fn_ = lua.create_function(move |lua_ctx, _: ()| {
            let points = reg.list_points();
            let json = serde_json::to_value(&points).unwrap_or(serde_json::Value::Array(Vec::new()));
            Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        contrib_table.set("list_points", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    arbor.set("contribution", contrib_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
