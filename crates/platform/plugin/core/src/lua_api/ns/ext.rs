//! `arbor.ext` — calling an installed extension.
//!
//! ## Why a plugin does the calling
//!
//! An extension answers; it does not decide. Which shader to translate, which mesh to build,
//! which of several backends to prefer — those are the plugin's, because the plugin is the
//! thing with a panel, a settings page and a user in front of it.
//!
//! That is also what keeps Arbor out of it. If the host routed to extensions itself it would
//! need to know that a shader has a mesh, and then the feature would be a built-in with a
//! wasm file attached rather than a plugin. **If the host has to learn something, it is not a
//! plugin.**
//!
//! ## Shape
//!
//! ```lua
//! local kinds = arbor.ext.call{
//!   interface = "mesh-source", id = "fulcrum", method = "catalogue",
//! }
//!
//! local mesh = arbor.ext.call{
//!   interface = "mesh-source", id = "fulcrum", method = "build",
//!   args = { "geode", '{"facets":9}' },
//! }
//! ```
//!
//! `args` is a **positional list**, because a component's type information carries parameter
//! types and not their names — there is nothing to key a table on. It reads better than it
//! sounds: the shapes inside are named, so a record argument is an ordinary Lua table keyed
//! by its fields.
//!
//! ## Bytes
//!
//! `call` answers in JSON, which is the wrong shape for a blob: a megabyte of payload becomes
//! six megabytes of number-array, serialised, parsed and held once in each process it crosses.
//! So two more calls exist for the case where the payload IS the point:
//!
//! ```lua
//! -- Download: the guest's bytes go straight to the file, never through Lua.
//! local written = arbor.ext.call_to_file{
//!   interface = "cloud-provider", id = "gcs", method = "read",
//!   args = { key, { start = 0, ["end"] = 8 * 1024 * 1024 } },
//!   path = dest, append = true,
//! }
//!
//! -- Upload: the file's bytes are lowered into argument 2.
//! arbor.ext.call_from_file{
//!   interface = "cloud-provider", id = "gcs", method = "write",
//!   args = { key, false, "application/octet-stream" }, file_arg = 2,
//!   path = src, offset = 0, length = 8 * 1024 * 1024,
//! }
//! ```
//!
//! `file_arg` is 1-based, like every other index a Lua author writes, and whatever `args`
//! holds at that position is ignored (`false` reads as a placeholder; `nil` cannot be used —
//! it would end the list).
//!
//! These are also what makes a large transfer *possible*: the caller ranges the reads and
//! appends the chunks, so neither process ever holds the whole object.
//!
//! ## The gate
//!
//! `service_call` in `[permissions]`. Calling an extension is invoking code from another
//! package, which is the permission that already means exactly that — and it matters here
//! rather than being ceremony: an installed cloud provider holds *its own* credentials, so a
//! plugin that could call it unasked could read somebody's bucket with a token that was never
//! granted to it.
//!
//! The two file calls need a second gate on top: they touch the local disk, so the path goes
//! through the same `fs` permission and scope check as `arbor.fs.*`, in the same place. The
//! check runs HERE and hands the shell an absolute path — the I/O happens in the shell
//! process, whose working directory is not this one's, so a relative path would otherwise mean
//! two different files.
//!
//! One flag is coarser than this deserves. A per-interface allowlist would say "this plugin
//! may call mesh sources and nothing else", which is the shape this wants to end at.

use mlua::{Lua, LuaSerdeExt, Table, Value};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;
use crate::lua_api::helpers::fs_perm::{check_fs_read, check_fs_write, FsPerm};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let t = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    let no_host = || {
        mlua::Error::RuntimeError("arbor.ext: this host cannot run extensions".to_string())
    };
    let denied = || {
        mlua::Error::RuntimeError(
            "arbor.ext: this plugin does not declare `service_call` in [permissions] — calling \
             an extension is invoking another package's code"
                .to_string(),
        )
    };

    // arbor.ext.list() → array of { interface, version, id, plugin, exports }
    {
        let plugin = ctx.plugin_name.clone();
        let app = ctx.app_ctx.clone();
        let allowed = ctx.service_call;
        let f = lua
            .create_function(move |lua, ()| {
                if !allowed {
                    return Err(denied());
                }
                let app = app.as_ref().ok_or_else(no_host)?;
                let json = app.ext_surface(&plugin).map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &json)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("list", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.ext.call{ interface=…, version=…, id=…, method=…, args={…}, export=… }
    {
        let plugin = ctx.plugin_name.clone();
        let app = ctx.app_ctx.clone();
        let allowed = ctx.service_call;
        let f = lua
            .create_function(move |lua, spec: Table| {
                if !allowed {
                    return Err(denied());
                }
                let app = app.as_ref().ok_or_else(no_host)?;
                // Serialised here rather than field by field: the whole point of this call is
                // that the shapes inside `args` are the extension's and not ours, so anything
                // that inspected them would be a place to get them wrong.
                let spec_json = serde_json::to_string(&lua_table_to_json(lua, &spec)?)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let out = app
                    .ext_call(&plugin, &spec_json)
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &out)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("call", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.ext.call_to_file{ …call spec…, path=…, append=… } → bytes written
    {
        let plugin = ctx.plugin_name.clone();
        let app = ctx.app_ctx.clone();
        let allowed = ctx.service_call;
        let fp: FsPerm = (ctx.fs_perm, ctx.fs_scope.clone());
        let f = lua
            .create_function(move |lua, spec: Table| {
                if !allowed {
                    return Err(denied());
                }
                let app = app.as_ref().ok_or_else(no_host)?;
                let (call_json, file_json) =
                    split_file_spec(lua, &spec, &fp, true, "arbor.ext.call_to_file")?;
                app.ext_call_to_file(&plugin, &call_json, &file_json)
                    .map_err(mlua::Error::RuntimeError)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("call_to_file", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    // arbor.ext.call_from_file{ …call spec…, path=…, file_arg=…, offset=…, length=… }
    {
        let plugin = ctx.plugin_name.clone();
        let app = ctx.app_ctx.clone();
        let allowed = ctx.service_call;
        let fp: FsPerm = (ctx.fs_perm, ctx.fs_scope.clone());
        let f = lua
            .create_function(move |lua, spec: Table| {
                if !allowed {
                    return Err(denied());
                }
                let app = app.as_ref().ok_or_else(no_host)?;
                let (call_json, file_json) =
                    split_file_spec(lua, &spec, &fp, false, "arbor.ext.call_from_file")?;
                let out = app
                    .ext_call_from_file(&plugin, &call_json, &file_json)
                    .map_err(mlua::Error::RuntimeError)?;
                json_to_lua(lua, &out)
            })
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        t.set("call_from_file", f).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    }

    arbor.set("ext", t).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

/// Split one Lua table into the two documents the host expects: where to call, and which file.
///
/// One table for the plugin author, because from where they stand it is one call. Two
/// documents on the wire, because the file half is the part the host is allowed to have an
/// opinion about — it is the half that touches the disk, and it is checked here against the
/// plugin's own `fs` permission before either of them leaves.
///
/// `writing` picks which gate applies: a download writes the local file, an upload reads it.
fn split_file_spec(
    lua: &Lua,
    spec: &Table,
    fp: &FsPerm,
    writing: bool,
    who: &str,
) -> mlua::Result<(String, String)> {
    let path: String = spec
        .get("path")
        .map_err(|_| mlua::Error::RuntimeError(format!("{who}: `path` is required")))?;
    let p = std::path::Path::new(&path);
    let abs = if writing { check_fs_write(lua, p, fp)? } else { check_fs_read(lua, p, fp)? };

    let file = serde_json::json!({
        "path":     abs.to_string_lossy(),
        "append":   spec.get::<Option<bool>>("append").unwrap_or(None).unwrap_or(false),
        "file_arg": spec.get::<Option<u64>>("file_arg").unwrap_or(None).unwrap_or(0),
        "offset":   spec.get::<Option<u64>>("offset").unwrap_or(None).unwrap_or(0),
        "length":   spec.get::<Option<u64>>("length").unwrap_or(None).unwrap_or(0),
    });

    // The call half is the same table minus the file keys. Removing them rather than building
    // a new table keeps this from having to know what a call spec contains — that is the
    // extension's business, and listing its fields here is how the two would drift.
    let call = lua_table_to_json(lua, spec)?;
    let mut call = match call {
        serde_json::Value::Object(map) => map,
        other => {
            return Err(mlua::Error::RuntimeError(format!(
                "{who}: expected a config table, got {other}"
            )))
        }
    };
    for key in ["path", "append", "file_arg", "offset", "length"] {
        call.remove(key);
    }

    Ok((
        serde_json::Value::Object(call).to_string(),
        file.to_string(),
    ))
}

/// Parse a JSON document the host produced and hand it back as Lua.
fn json_to_lua(lua: &Lua, s: &str) -> mlua::Result<Value> {
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
    lua.to_value(&v)
}

/// Convert one Lua table to JSON.
///
/// Through `serde_json` rather than by hand, so an empty Lua table takes the same road as
/// everywhere else — the `{}`-is-an-object trap is the plugin surface's oldest, and a second
/// conversion written here would be a second place for it to bite.
fn lua_table_to_json(lua: &Lua, t: &Table) -> mlua::Result<serde_json::Value> {
    lua.from_value(Value::Table(t.clone()))
        .map_err(|e| mlua::Error::RuntimeError(format!("arbor.ext: {e}")))
}
