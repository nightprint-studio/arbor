//! `arbor.ui.form` — callable table.
//!
//! Calling `arbor.ui.form(config)` opens a new form (existing behaviour,
//! preserved via the `__call` metamethod). The table also exposes helpers
//! to mutate fields in the currently-open form:
//!
//!   arbor.ui.form.set_options(name, opts)
//!   arbor.ui.form.set_disabled(name, bool)
//!   arbor.ui.form.set_value(name, value)
//!   arbor.ui.form.replace(partial_cfg)
//!
//! Each helper emits `plugin:form-update`; the modal applies the op only
//! when the open form belongs to this plugin.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let form_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    let open_fn = build_open_fn(ctx, lua)?;
    install_set_options(ctx, lua, &form_table)?;
    install_set_disabled(ctx, lua, &form_table)?;
    install_set_value(ctx, lua, &form_table)?;
    install_replace(ctx, lua, &form_table)?;
    install_set_loading(ctx, lua, &form_table)?;
    install_close(ctx, lua, &form_table)?;

    // Attach __call metatable so arbor.ui.form(config) still works.
    let meta = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    meta.set("__call", open_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    let _ = form_table.set_metatable(Some(meta));

    ui.set("form", form_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn build_open_fn(ctx: &ApiCtx, lua: &Lua) -> Result<mlua::Function> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    lua.create_function(move |lua_ctx, (_self, config): (mlua::Value, mlua::Table)| {
        let json_val: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(config))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let mut payload = serde_json::json!({ "plugin_name": pname });
        if let serde_json::Value::Object(ref obj) = json_val {
            for (k, v) in obj { payload[k] = v.clone(); }
        }
        if let Some(ref h) = handle { let _ = h.emit("plugin:form", payload); }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))
}

fn install_set_options(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (name, value): (String, mlua::Value)| {
        let payload_json: serde_json::Value = lua_ctx
            .from_value(value)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_options",
                "name":        name,
                "payload":     payload_json,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("set_options", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_disabled(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, (name, disabled): (String, bool)| {
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_disabled",
                "name":        name,
                "payload":     disabled,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("set_disabled", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_value(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (name, value): (String, mlua::Value)| {
        let payload_json: serde_json::Value = lua_ctx
            .from_value(value)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_value",
                "name":        name,
                "payload":     payload_json,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("set_value", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_replace(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // replace(partial_cfg)
    // Replaces the nodes (and optionally state) of the currently-open
    // form in-place, preserving field values whose `name` still exists
    // in the new structure. `partial_cfg` is a table with:
    //   { nodes = { ... }, state = { ... optional ... } }
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| {
        let payload_json: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(cfg))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "replace",
                "payload":     payload_json,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("replace", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_loading(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // set_loading(true)
    // set_loading(false)
    // set_loading({ loading = true, label = "Fetching 3/12…" })
    // set_loading("Fetching 3/12…")   -- shorthand: implies loading = true
    //
    // Lightweight progress hook — flips the modal's translucent overlay
    // and (optionally) updates its label without re-rendering the entire
    // form node tree. Use during a tight fan-out loop where issuing a
    // full `replace` per step would just re-mount the form for nothing.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, arg: mlua::Value| {
        let (loading, label): (bool, Option<String>) = match arg {
            mlua::Value::Boolean(b) => (b, None),
            mlua::Value::String(s)  => (true, Some(s.to_str()?.to_string())),
            mlua::Value::Nil        => (false, None),
            mlua::Value::Table(t)   => {
                let l: bool = t.get::<Option<bool>>("loading").ok().flatten().unwrap_or(true);
                let lbl: Option<String> = t.get::<Option<String>>("label").ok().flatten();
                (l, lbl)
            }
            _ => return Err(mlua::Error::RuntimeError(
                "arbor.ui.form.set_loading: expected bool, string, table or nil".into()
            )),
        };
        let _ = lua_ctx;  // unused; the helper closes over the host handle only
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_loading",
                "loading":     loading,
                "label":       label,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("set_loading", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_close(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // close()
    // Programmatically dismiss the currently-open form belonging to this
    // plugin. Pairs with `keep_open = true` on the form config: when the
    // submit handler launches a follow-up flow (file picker, confirm,
    // second form) the original form stays mounted, and the plugin
    // calls form.close() once that flow completes.
    //
    // Frontend listens via `plugin:form-update { op = "close" }` —
    // PluginFormModal calls its onClose prop when this op arrives and
    // the open form belongs to this plugin.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, ()| {
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "close",
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    form_table.set("close", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
