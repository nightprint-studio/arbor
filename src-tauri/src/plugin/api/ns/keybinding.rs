//! `arbor.keybinding.register` — plugin keyboard shortcuts.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::dual_write_contribution;
use crate::plugin::contribution::points;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let kb_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_register(ctx, lua, &kb_table)?;

    arbor.set("keybinding", kb_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register(ctx: &ApiCtx, lua: &Lua, kb_table: &Table) -> Result<()> {
    // register({ key, ctrl?, shift?, alt?, action, description? })
    // — sugar for arbor.ui.contribute("arbor:keybinding", …)
    let pname = ctx.plugin_name.clone();
    let contribs = ctx.contributions.clone();
    let handle   = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let action = config.get::<String>("action").map_err(|_| {
            mlua::Error::RuntimeError("arbor.keybinding.register: 'action' is required".to_string())
        })?;
        let key = config.get::<String>("key").map_err(|_| {
            mlua::Error::RuntimeError("arbor.keybinding.register: 'key' is required".to_string())
        })?;
        let ctrl  = config.get::<bool>("ctrl").unwrap_or(false);
        let shift = config.get::<bool>("shift").unwrap_or(false);
        let alt   = config.get::<bool>("alt").unwrap_or(false);
        let description = config.get::<String>("description").unwrap_or_default();
        // item_id derived from the chord — re-registering the same
        // shortcut updates description in place.
        let chord = format!(
            "{}{}{}{}",
            if ctrl  { "Ctrl+"  } else { "" },
            if shift { "Shift+" } else { "" },
            if alt   { "Alt+"   } else { "" },
            key.as_str(),
        );
        let payload = serde_json::json!({
            "key":         key,
            "ctrl":        ctrl,
            "shift":       shift,
            "alt":         alt,
            "action":      action,
            "description": description,
        });
        dual_write_contribution(
            &contribs, &handle, &pname,
            points::KEYBINDING, &chord, payload, 100,
        );
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    kb_table.set("register", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
