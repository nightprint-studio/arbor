//! `arbor.toolchain` (toolchain registry CRUD + env resolution), ported to run
//! through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/toolchain.rs`: same namespace (`arbor.toolchain`), same function
//! names (`list` / `active` / `env` / `detect` / `add` / `remove` /
//! `set_active`), same argument shapes, same `(value, err)` / `(true|false, err)`
//! tuple conventions, same permission-gate `RuntimeError` strings, same
//! `toolchain.<op>[ lock| encode]: …` error prefixes.
//!
//! This is a **PROXY** namespace: the toolchain registry lives in the shell's
//! `AppState` (`toolchain_registry`), not in `corvus-be`. So unlike the
//! repo/notes/git namespaces (which open a repo by path and do the work
//! in-process), every op here goes through the captured `Arc<dyn NsHost>` whose
//! `corvus-be` impl calls back over the reverse channel
//! (`host_call("__toolchain_<op>", …)`); the matching shell handler in
//! `src-tauri/src/ipc/mod.rs` reads/mutates the real `AppState` registry exactly
//! as `ns_shell/toolchain.rs` did. The registry is **not** repo-scoped — it is a
//! single global, so none of these read `__arbor_current_repo__`.
//!
//! Calling convention (unchanged from the shell — see `ns_shell/toolchain.rs`):
//!   · `list(kind)` / `active(kind)` / `detect(kind)` are positional, returning
//!     `(value, nil) | (nil, err)`; `active` returns `(nil, nil)` when there is no
//!     active entry.
//!   · `env{kind, id?}` is a table-config, returning `(env_map, nil) | (nil, err)`.
//!   · `add(kind, entry_table)` validates the entry table installer-side (a bad
//!     shape raises), then returns `(true, nil) | (false, err)`.
//!   · `remove(kind, id)` / `set_active(kind, id)` are positional, returning the
//!     bool tuple.
//! Requires toolchain ≥ Read for `list`/`active`/`env`/`detect`, toolchain ≥
//! Write for `add`/`remove`/`set_active`.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, err2, json_to_lua, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.toolchain.*` installer. Holds the host handle the closures call through.
pub struct ToolchainInstaller {
    host: NsHostHandle,
}

impl ToolchainInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for ToolchainInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let t = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_list(self.host.clone(), ctx, lua, &t)?;
        install_active(self.host.clone(), ctx, lua, &t)?;
        install_env(self.host.clone(), ctx, lua, &t)?;
        install_detect(self.host.clone(), ctx, lua, &t)?;
        install_add(self.host.clone(), ctx, lua, &t)?;
        install_remove(self.host.clone(), ctx, lua, &t)?;
        install_set_active(self.host.clone(), ctx, lua, &t)?;

        arbor
            .set("toolchain", t)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_list(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, kind: String| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.list: toolchain = \"read\" (or higher) permission required"
                        .to_string(),
                ));
            }
            // Host returns the serde-serialized `Vec<ToolchainEntry>` as a JSON array.
            let json = match host.toolchain_list(&kind) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_active(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, kind: String| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.active: toolchain = \"read\" (or higher) permission required"
                        .to_string(),
                ));
            }
            // Host returns `Ok(Some(entry_json))` when an entry is active,
            // `Ok(None)` when none is (→ Lua nil), exactly as the shell's
            // `g.active(&kind)` → `None | Some(e)` split.
            match host.toolchain_active(&kind) {
                Ok(Some(json)) => ok2(lua_ctx, json_to_lua(lua_ctx, &json)?),
                Ok(None) => ok2(lua_ctx, mlua::Value::Nil),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("active", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_env(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.env: toolchain = \"read\" (or higher) permission required"
                        .to_string(),
                ));
            }
            let kind: String = cfg.get("kind").map_err(|_| {
                mlua::Error::RuntimeError("arbor.toolchain.env: 'kind' is required".into())
            })?;
            let id: Option<String> = cfg.get::<Option<String>>("id").unwrap_or(None);

            // Host returns the serde-serialized `HashMap<String, String>` env map.
            let json = match host.toolchain_env(&kind, id.as_deref()) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("env", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_detect(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let read = ctx.toolchain_read || ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, kind: String| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.detect: toolchain = \"read\" (or higher) permission required"
                        .to_string(),
                ));
            }
            // Host returns the serde-serialized `Vec<ToolchainEntry>` (newly
            // discovered, not yet added).
            let json = match host.toolchain_detect(&kind) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            ok2(lua_ctx, json_to_lua(lua_ctx, &json)?)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("detect", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_add(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let write = ctx.toolchain_write;
    let fn_ = lua
        .create_function(
            move |lua_ctx, (kind, entry_table): (String, mlua::Table)| -> LuaTuple {
                if !write {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.toolchain.add: toolchain = \"write\" permission required".to_string(),
                    ));
                }
                // Validate the entry shape installer-side against the real
                // `ToolchainEntry` (a bad table raises with the shell's exact
                // `arbor.toolchain.add: invalid entry: …` message), then re-encode
                // to JSON for the host — the shell handler deserializes it back into
                // the typed `ToolchainEntry`. Validating here (not just shell-side)
                // keeps the raise-on-bad-shape semantics byte-for-byte.
                let entry: arbor_plugin_core::prelude::ToolchainEntry = lua_ctx
                    .from_value(mlua::Value::Table(entry_table))
                    .map_err(|e| {
                        mlua::Error::RuntimeError(format!(
                            "arbor.toolchain.add: invalid entry: {e}"
                        ))
                    })?;
                let entry_json = serde_json::to_value(&entry).map_err(|e| {
                    mlua::Error::RuntimeError(format!(
                        "arbor.toolchain.add: encode entry: {e}"
                    ))
                })?;
                match host.toolchain_add(&kind, entry_json) {
                    Ok(()) => boolerr2(lua_ctx, true, None),
                    Err(e) => boolerr2(lua_ctx, false, Some(e)),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("add", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_remove(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let write = ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, (kind, id): (String, String)| -> LuaTuple {
            if !write {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.remove: toolchain = \"write\" permission required".to_string(),
                ));
            }
            match host.toolchain_remove(&kind, &id) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("remove", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_active(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let write = ctx.toolchain_write;
    let fn_ = lua
        .create_function(move |lua_ctx, (kind, id): (String, String)| -> LuaTuple {
            if !write {
                return Err(mlua::Error::RuntimeError(
                    "arbor.toolchain.set_active: toolchain = \"write\" permission required"
                        .to_string(),
                ));
            }
            match host.toolchain_set_active(&kind, &id) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("set_active", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
