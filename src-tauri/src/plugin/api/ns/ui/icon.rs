//! `arbor.ui.icon.register({id, svg})`.
//!
//! Stores a raw SVG string keyed by `<plugin>:<id>`. Reference it from any
//! `icon` field as `"plugin:<plugin>:<id>"` (PluginIcon.svelte resolves it).
//! The registry is wiped on plugin reload/disable so stale icons never leak.

use mlua::{Lua, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::dual_write_contribution;
use crate::plugin::contribution::points;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let icon_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    let pname = ctx.plugin_name.clone();
    let reg = ctx.icon_registry.clone();
    let handle = ctx.app_handle.clone();
    let contribs_icon = ctx.contributions.clone();
    let register_fn = lua.create_function(move |_, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError(
                "arbor.ui.icon.register: 'id' is required".to_string()
            )
        })?;
        let svg = config.get::<String>("svg").map_err(|_| {
            mlua::Error::RuntimeError(
                "arbor.ui.icon.register: 'svg' is required".to_string()
            )
        })?;
        // Dual-write: same SVG also into the unified registry, keyed by id.
        // Frontend consumers read it via `customIcon()` on the contribution
        // store and refresh on the coalesced `arbor://contributions-changed`
        // event — the legacy `arbor://icons-changed` event is gone.
        let payload = serde_json::json!({ "svg": svg.clone() });
        dual_write_contribution(
            &contribs_icon, &handle, &pname,
            points::ICON, &id, payload, 100,
        );
        reg.register(&pname, &id, svg);
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    icon_table.set("register", register_fn).map_err(|e| AppError::Plugin(e.to_string()))?;

    ui.set("icon", icon_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
