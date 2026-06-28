//! `arbor.linked_worktrees` (read + sync-toggle access to the cross-repo
//! worktree-link registry), ported to run through an [`NsHost`] instead of a
//! `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/linked_worktrees.rs`: same namespace (`arbor.linked_worktrees`),
//! same function names (`list` / `get` / `set_sync_enabled`), same argument
//! shapes, same return conventions, same `set_sync_enabled: …` error prefix.
//! The only difference is *where the work goes*:
//!
//!   · the shell read/wrote the shared `linked_worktrees.toml` directly via
//!     `crate::linked_worktrees::{load, get, set_sync_enabled, save}` and emitted
//!     `arbor://worktree-links-changed` through its `tauri::AppHandle`;
//!   · here the work goes through the captured `Arc<dyn NsHost>`, whose impl in
//!     `corvus-be` reaches the **same** file-backed `linked_worktrees.toml`
//!     (via `be::worktree_links`, which reloads-on-access) and emits the same
//!     `arbor://worktree-links-changed` event through corvus-be's event sink.
//!
//! Unlike the git namespaces this surface is **not** repo-scoped — the registry
//! is a single global file, so there is no `__arbor_current_repo__` read and no
//! `git`-permission gate (mirroring the shell, which gated none of these three).
//!
//! Calling convention (unchanged):
//!   · `list()` → a Lua array of `{ id, name, sync_enabled, member_count }` rows
//!     (a deliberate projection, not the full link record). No `(value, err)`
//!     tuple — it returns the table directly, or `(nil, err)` on a host failure.
//!   · `get(id)` → the full serde-serialized link record, or `nil` when absent;
//!     `(nil, err)` on a host failure.
//!   · `set_sync_enabled(id, enabled)` → the `(ok, err)` bool tuple.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.linked_worktrees.*` installer. Holds the host handle the closures call
/// through.
pub struct LinkedWorktreesInstaller {
    host: NsHostHandle,
}

impl LinkedWorktreesInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for LinkedWorktreesInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let lw_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_list(self.host.clone(), ctx, lua, &lw_table)?;
        install_get(self.host.clone(), ctx, lua, &lw_table)?;
        install_set_sync_enabled(self.host.clone(), ctx, lua, &lw_table)?;

        arbor
            .set("linked_worktrees", lw_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_list(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    lw_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ()| -> LuaTuple {
            // Host returns the projected array `[{id, name, sync_enabled,
            // member_count}, …]` (same shape the shell built row-by-row).
            let json = match host.linked_worktrees_list() {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("linked_worktrees.list to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    lw_table
        .set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    lw_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, id: String| -> LuaTuple {
            match host.linked_worktrees_get(&id) {
                Ok(Some(json)) => match lua_ctx.to_value(&json) {
                    Ok(v) => ok2(lua_ctx, v),
                    Err(e) => err2(lua_ctx, format!("linked_worktrees.get to_value: {e}")),
                },
                Ok(None) => ok2(lua_ctx, mlua::Value::Nil),
                Err(e) => err2(lua_ctx, e),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    lw_table
        .set("get", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_sync_enabled(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    lw_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(
            move |lua_ctx, (id, enabled): (String, bool)| -> LuaTuple {
                // The host mutates + persists the shared file and emits
                // `arbor://worktree-links-changed`, exactly as the shell did
                // inline after `save`. Error text keeps the `set_sync_enabled: …`
                // prefix the shell produced.
                match host.linked_worktrees_set_sync_enabled(&id, enabled) {
                    Ok(()) => boolerr2(lua_ctx, true, None),
                    Err(e) => boolerr2(lua_ctx, false, Some(format!("set_sync_enabled: {e}"))),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    lw_table
        .set("set_sync_enabled", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
