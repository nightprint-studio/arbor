//! Cross-plugin contribution slots.
//!
//!   arbor.ui.contribute(point, item)
//!   arbor.ui.contribute_patch(point, id, partial)
//!   arbor.ui.unregister_contribution(point, item_id)
//!   arbor.ui.contribution_point({name, description?, schema?})
//!   arbor.ui.list_contributions(point)
//!
//! `item = {id, payload, priority?, when?, disabled?, group?}`.
//! Re-contributing with the same id replaces the previous payload.

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::contrib_write::contribute_patch_payload;
use crate::plugin::contribution::{
    ContributionPoint, PluginContribution, WhenClause, validate_built_in,
};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    install_contribute(ctx, lua, ui)?;
    install_contribute_patch(ctx, lua, ui)?;
    install_unregister(ctx, lua, ui)?;
    install_declare_point(ctx, lua, ui)?;
    install_list_contributions(ctx, lua, ui)?;
    Ok(())
}

fn install_contribute(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let reg = ctx.contributions.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (point, item): (String, mlua::Table)| {
        let item_id = item.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.contribute: 'id' is required".to_string())
        })?;
        let priority = item.get::<i32>("priority").unwrap_or(100);
        let payload_value = item.get::<mlua::Value>("payload").unwrap_or(mlua::Value::Nil);

        // ── Top-level Phase 5 fields ────────────────────────────────────
        // `when`, `disabled`, `group` were promoted from informal payload
        // conventions to typed top-level item fields. We read both forms
        // and prefer the top-level one; if only the legacy nested form is
        // present, we migrate it (and warn so the plugin author can clean
        // up — perpetual warns aren't great but the cleanup is a one-line
        // edit per call site).
        let when_top      = item.get::<mlua::Value>("when").unwrap_or(mlua::Value::Nil);
        let disabled_top  = item.get::<Option<bool>>("disabled").unwrap_or(None);
        let group_top     = item.get::<Option<String>>("group").unwrap_or(None);

        // Payload defaults to the whole item (minus reserved keys) if not given.
        let mut payload: serde_json::Value = match payload_value {
            mlua::Value::Nil => serde_json::to_value(&item).unwrap_or(serde_json::Value::Null),
            v => serde_json::to_value(&v).unwrap_or(serde_json::Value::Null),
        };

        let mut when: Option<WhenClause> = match when_top {
            mlua::Value::Nil => None,
            v => {
                let json = serde_json::to_value(&v).unwrap_or(serde_json::Value::Null);
                serde_json::from_value(json).ok()
            }
        };
        let mut disabled = disabled_top.unwrap_or(false);
        let mut group    = group_top;

        lift_legacy_payload_fields(
            &pname, &point, &item_id,
            &mut payload, &mut when, &mut disabled, &mut group,
            disabled_top.is_some(),
        );

        // ── Schema validation for built-in points ───────────────────────
        // Plugin-defined points fall through (no schema known); built-in
        // ones must match their documented shape or the contribution is
        // dropped.
        if let Err(e) = validate_built_in(&point, &payload) {
            tracing::error!(
                target: "plugin",
                plugin = %pname, point = %point, item_id = %item_id,
                "contribution rejected — schema validation failed: {}", e,
            );
            return Ok(());
        }

        let contribution = PluginContribution {
            plugin_name: pname.to_string(),
            point:       point.clone(),
            item_id:     item_id.clone(),
            payload,
            priority,
            when, disabled, group,
        };
        reg.contribute(contribution);
        reg.notify_changed(&handle, &point);
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("contribute", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Migrate legacy `payload.when` / `payload.disabled` / `payload.group`
/// to top-level fields. Logs a deprecation warning at most once per call.
fn lift_legacy_payload_fields(
    pname: &str,
    point: &str,
    item_id: &str,
    payload: &mut serde_json::Value,
    when: &mut Option<WhenClause>,
    disabled: &mut bool,
    group: &mut Option<String>,
    disabled_top_set: bool,
) {
    let group_top_set = group.is_some();
    if let Some(obj) = payload.as_object_mut() {
        if let Some(legacy) = obj.remove("when") {
            if when.is_some() {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     both top-level `when` and `payload.when` set; ignoring `payload.when`",
                );
            } else {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     `payload.when` is deprecated — move to top-level `when`",
                );
                *when = serde_json::from_value(legacy).ok();
            }
        }
        if let Some(legacy) = obj.remove("disabled") {
            if disabled_top_set {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     both top-level `disabled` and `payload.disabled` set; ignoring `payload.disabled`",
                );
            } else {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     `payload.disabled` is deprecated — move to top-level `disabled`",
                );
                *disabled = legacy.as_bool().unwrap_or(false);
            }
        }
        if let Some(legacy) = obj.remove("group") {
            if group_top_set {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     both top-level `group` and `payload.group` set; ignoring `payload.group`",
                );
            } else {
                tracing::warn!(
                    target: "plugin",
                    "[{pname}] contribution to '{point}' (id='{item_id}'): \
                     `payload.group` is deprecated — move to top-level `group`",
                );
                *group = legacy.as_str().map(String::from);
            }
        }
    }
}

fn install_contribute_patch(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // arbor.ui.contribute_patch(point, id, partial)
    //
    // Shallow-merge `partial` into the existing payload at (this plugin, point,
    // id) and write back. If no prior payload exists, `partial` becomes the
    // whole payload. Useful when a plugin wants to update one or two fields of
    // a previously-contributed item (e.g. a combo's `options` list) without
    // re-specifying the full payload. `set_combo_options` is a thin sugar over
    // this.
    let pname = ctx.plugin_name.clone();
    let reg = ctx.contributions.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (point, item_id, partial): (String, String, mlua::Value)| {
        let partial_json: serde_json::Value =
            serde_json::to_value(&partial).unwrap_or(serde_json::Value::Null);
        contribute_patch_payload(&reg, &handle, &pname, &point, &item_id, partial_json, 100);
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("contribute_patch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_unregister(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let reg = ctx.contributions.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (point, item_id): (String, String)| {
        let removed = reg.remove(&pname, &point, &item_id);
        if removed {
            reg.notify_changed(&handle, &point);
        }
        Ok(removed)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("unregister_contribution", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_declare_point(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let pname = ctx.plugin_name.clone();
    let reg = ctx.contributions.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let name = config.get::<String>("name").map_err(|_| {
            mlua::Error::RuntimeError(
                "arbor.ui.contribution_point: 'name' is required".to_string()
            )
        })?;
        let description = config.get::<Option<String>>("description").unwrap_or(None);
        let schema_v = config.get::<mlua::Value>("schema").unwrap_or(mlua::Value::Nil);
        let schema = match schema_v {
            mlua::Value::Nil => None,
            v => Some(serde_json::to_value(&v).unwrap_or(serde_json::Value::Null)),
        };
        reg.declare_point(ContributionPoint {
            plugin_name: pname.to_string(),
            name, description, schema,
        });
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("contribution_point", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_contributions(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // Convenience reader so a host plugin can iterate a point's
    // contributions from Lua (validate them, fold them into its own tree
    // snapshot, …). Reads are open by design — any plugin can list any
    // point's items, the contribution model is intentionally transparent.
    let reg = ctx.contributions.clone();
    let fn_ = lua.create_function(move |lua_ctx, point: String| {
        let items = reg.list_for_point(&point);
        let json = serde_json::to_value(&items).unwrap_or(serde_json::Value::Array(Vec::new()));
        Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("list_contributions", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
