//! `arbor.ui.form` — callable table.
//!
//! Calling `arbor.ui.form(config)` opens a new form (existing behaviour,
//! preserved via the `__call` metamethod). The table also exposes helpers
//! to mutate fields in the currently-open form:
//!
//!   arbor.ui.form.set_options(name, opts)            -- legacy
//!   arbor.ui.form.set_options({ id|name, options })  -- explicit
//!   arbor.ui.form.set_disabled(name, bool)           -- legacy
//!   arbor.ui.form.set_disabled({ id|name, disabled })
//!   arbor.ui.form.set_value(name, value)             -- legacy
//!   arbor.ui.form.set_value({ id|name, value })      -- explicit
//!   arbor.ui.form.replace(partial_cfg)
//!   arbor.ui.form.patch(ops)               -- granular node-tree mutations
//!   arbor.ui.form.set_state_path(segs, v)  -- mutate one liveState slice
//!
//! Both shapes are accepted on `set_value` / `set_options` / `set_disabled`:
//!   - `(name, payload)` — legacy positional shortcut (field NAME, not node id).
//!   - `{ name = "…", <payload_key> = … }` — explicit cfg form, NAME-keyed.
//!   - `{ id = "…",   <payload_key> = … }` — explicit cfg form, NODE-ID-keyed
//!     (the renderer resolves id → field name by walking the node tree).
//! `<payload_key>` is `value` / `options` / `disabled` respectively. Picking
//! `id` over `name` is the recommended pattern when the calling code already
//! tracks the node id (the same key it uses for `patch`), removing the need
//! to remember a separate field-name table.
//!
//! Each helper emits `plugin:form-update`; the modal applies the op only
//! when the open form belongs to this plugin.

use mlua::{Lua, LuaSerdeExt, MultiValue, Table};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

/// Parsed `(name, payload)` / cfg-table for set_value / set_options / set_disabled.
/// Exactly one of `name` / `id` is guaranteed to be `Some` (the other may be `None`).
struct SetArgs {
    name:    Option<String>,
    id:      Option<String>,
    payload: serde_json::Value,
}

/// Accept either `(name_str, payload_value)` or a single cfg table
/// `{ name | id, <payload_key> = ... }`. `payload_key` is the explicit field
/// name inside the cfg table (`value` / `options` / `disabled`).
fn parse_set_args(
    lua: &Lua,
    args: MultiValue,
    payload_key: &'static str,
) -> mlua::Result<SetArgs> {
    let usage = || format!(
        "expected (name, {p}) or cfg table {{ id|name, {p} }}",
        p = payload_key
    );
    let mut iter = args.into_iter();
    let first = iter.next().ok_or_else(|| mlua::Error::RuntimeError(usage()))?;
    let second = iter.next();
    match first {
        mlua::Value::String(s) => {
            let name = s.to_str()?.to_string();
            let value = second.unwrap_or(mlua::Value::Nil);
            let payload: serde_json::Value = lua.from_value(value)?;
            Ok(SetArgs { name: Some(name), id: None, payload })
        }
        mlua::Value::Table(t) => {
            let name = t.get::<Option<String>>("name").ok().flatten();
            let id   = t.get::<Option<String>>("id").ok().flatten();
            if name.is_none() && id.is_none() {
                return Err(mlua::Error::RuntimeError(
                    "cfg table must carry either `id` or `name`".into()
                ));
            }
            let value: mlua::Value = t.get(payload_key)?;
            let payload: serde_json::Value = lua.from_value(value)?;
            Ok(SetArgs { name, id, payload })
        }
        _ => Err(mlua::Error::RuntimeError(usage())),
    }
}

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let form_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let open_fn = build_open_fn(ctx, lua)?;
    install_set_options(ctx, lua, &form_table)?;
    install_set_disabled(ctx, lua, &form_table)?;
    install_set_value(ctx, lua, &form_table)?;
    install_replace(ctx, lua, &form_table)?;
    install_patch(ctx, lua, &form_table)?;
    install_set_state_path(ctx, lua, &form_table)?;
    install_set_loading(ctx, lua, &form_table)?;
    install_close(ctx, lua, &form_table)?;

    // Attach __call metatable so arbor.ui.form(config) still works.
    let meta = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    meta.set("__call", open_fn).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    let _ = form_table.set_metatable(Some(meta));

    ui.set("form", form_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn build_open_fn(ctx: &ApiCtx, lua: &Lua) -> Result<mlua::Function> {
    let handle = ctx.app_ctx.clone();
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
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))
}

fn install_set_options(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, args: MultiValue| {
        let parsed = parse_set_args(lua_ctx, args, "options")
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.ui.form.set_options: {}", e)))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_options",
                "name":        parsed.name,
                "id":          parsed.id,
                "payload":     parsed.payload,
            }));
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("set_options", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_disabled(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, args: MultiValue| {
        let parsed = parse_set_args(lua_ctx, args, "disabled")
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.ui.form.set_disabled: {}", e)))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_disabled",
                "name":        parsed.name,
                "id":          parsed.id,
                "payload":     parsed.payload,
            }));
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("set_disabled", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_value(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, args: MultiValue| {
        let parsed = parse_set_args(lua_ctx, args, "value")
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.ui.form.set_value: {}", e)))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "set_value",
                "name":        parsed.name,
                "id":          parsed.id,
                "payload":     parsed.payload,
            }));
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("set_value", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_replace(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // replace(partial_cfg)
    // Replaces the nodes (and optionally state) of the currently-open
    // form in-place, preserving field values whose `name` still exists
    // in the new structure. `partial_cfg` is a table with:
    //   { nodes = { ... }, state = { ... optional ... } }
    let handle = ctx.app_ctx.clone();
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
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("replace", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_patch(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // patch(ops)
    // Granular, in-place mutations of the currently-open form's node tree —
    // sibling to `replace`, but surgical (no re-mount). `ops` is an array of
    // tables, each addressing a node by its stable `id` plus one verb:
    //   { id = "...", merge  = { ...props } }          -- shallow-merge props
    //   { id = "...", set    = { "options", 1, "label" }, value = ... }
    //   { id = "...", append = { ...node }, to = "children" }  -- to defaults to "children"
    //   { id = "...", remove = true }                  -- splice the node out
    // A node without a stable `id` can't be patched (use `replace`).
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, ops: mlua::Table| {
        let patches: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(ops))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "patch",
                "patches":     patches,
            }));
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("patch", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_state_path(ctx: &ApiCtx, lua: &Lua, form_table: &Table) -> Result<()> {
    // set_state_path(segments, value)
    // Mutate a single slice of the form's opaque liveState without replacing
    // the whole blob (sibling to `replace { state = ... }`). `segments` is an
    // array of string/number keys, e.g. { "filters", "branch" }. A `nil`
    // value DELETES the addressed key (there is no JSON-null literal in Lua,
    // so nil unambiguously means "drop it").
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (path, value): (mlua::Table, mlua::Value)| {
        let path_json: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(path))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let mut payload = serde_json::json!({
            "plugin_name": pname,
            "op":          "set_state_path",
            "path":        path_json,
        });
        if matches!(value, mlua::Value::Nil) {
            payload["delete"] = serde_json::Value::Bool(true);
        } else {
            payload["value"] = lua_ctx
                .from_value(value)
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", payload);
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("set_state_path", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
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
    let handle = ctx.app_ctx.clone();
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
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("set_loading", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
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
    let handle = ctx.app_ctx.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, ()| {
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:form-update", serde_json::json!({
                "plugin_name": pname,
                "op":          "close",
            }));
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    form_table.set("close", fn_).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
