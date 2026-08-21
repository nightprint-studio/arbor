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
//! ## The gate
//!
//! `service_call` in `[permissions]`. Calling an extension is invoking code from another
//! package, which is the permission that already means exactly that — and it matters here
//! rather than being ceremony: an installed cloud provider holds *its own* credentials, so a
//! plugin that could call it unasked could read somebody's bucket with a token that was never
//! granted to it.
//!
//! One flag is coarser than this deserves. A per-interface allowlist would say "this plugin
//! may call mesh sources and nothing else", which is the shape this wants to end at.

use mlua::{Lua, LuaSerdeExt, Table, Value};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;

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

    arbor.set("ext", t).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
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
