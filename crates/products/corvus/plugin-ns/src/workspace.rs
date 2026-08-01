//! `arbor.workspace` (workspace / repo-registry queries), ported to run through
//! an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/workspace.rs`: same namespace (`arbor.workspace`), same function
//! names (`list` / `active` / `get` / `list_repos` / `repo` / `switch`), same
//! argument shapes, same return conventions, same error strings. The only
//! difference is *where the work goes*:
//!
//!   · the shell downcast `ApiCtx::app_handle()` → `tauri::AppState` and locked
//!     `lock_workspaces()` / `lock_repo_registry()` directly;
//!   · here the work goes through the captured `Arc<dyn NsHost>`, which reads the
//!     corvus-be `workspace::{store,registry}` (reload-on-access) and returns the
//!     same JSON shapes — so results and error text match.
//!
//! ## Calling conventions (unchanged from the shell)
//!
//! The **read** functions return a *single* value (a table or `nil`), NOT a
//! `(value, err)` tuple — they swallow lock/missing errors into `nil` exactly as
//! the shell did:
//!   · `list()`              → array of workspace tables (`[]` when none).
//!   · `active()`            → workspace table, or `nil`.
//!   · `get(ws_id)`          → workspace table, or `nil`.
//!   · `list_repos(ws_id?)`  → array of repo-entry tables; the whole registry
//!     when `ws_id` is omitted, else just that workspace's members (`[]` when the
//!     workspace is unknown).
//!   · `repo(repo_id)`       → repo-entry table, or `nil`.
//!
//! Only `switch(ws_id)` returns the bool tuple `(true, nil) | (false, err)`; on
//! success the host fires `corvus:workspace_switched` and emits
//! `arbor://workspace-switched`, exactly as the shell did inline.
//!
//! ## Lua table shapes (preserved exactly)
//!
//! The shell hand-built these tables (`ws_to_lua` / `entry_to_lua`); here the
//! host returns the equivalent JSON and the installer feeds it to `lua.to_value`.
//! A workspace table carries `{ id, name, color_idx, group_id, repo_ids,
//! repo_count }`; a repo-entry table carries `{ id, path, display_name,
//! remote_url }`. The host MUST serialize exactly those fields so the Lua surface
//! is identical.
//!
//! No permission gate: the shell's `workspace` namespace had none (workspace
//! identity is host-level, not repo git-scope), so neither does this port.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.workspace.*` installer. Holds the host handle the closures call through.
pub struct WorkspaceInstaller {
    host: NsHostHandle,
}

impl WorkspaceInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for WorkspaceInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let ws_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_list(self.host.clone(), ctx, lua, &ws_table)?;
        install_active(self.host.clone(), ctx, lua, &ws_table)?;
        install_get(self.host.clone(), ctx, lua, &ws_table)?;
        install_list_repos(self.host.clone(), ctx, lua, &ws_table)?;
        install_repo(self.host.clone(), ctx, lua, &ws_table)?;
        install_switch(self.host.clone(), ctx, lua, &ws_table)?;

        arbor
            .set("workspace", ws_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

// ─── Functions ───────────────────────────────────────────────────────────────

fn install_list(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ()| {
            // Lock/error → empty array, matching the shell's `?`-swallow-into-nil.
            let json = match host.workspace_list() {
                Ok(v) => v,
                Err(_) => return Ok(mlua::Value::Table(lua_ctx.create_table()?)),
            };
            lua_ctx.to_value(&json)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_active(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ()| match host.workspace_active() {
            Ok(Some(v)) => lua_ctx.to_value(&v),
            _ => Ok(mlua::Value::Nil),
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("active", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ws_id: String| match host.workspace_get(&ws_id) {
            Ok(Some(v)) => lua_ctx.to_value(&v),
            _ => Ok(mlua::Value::Nil),
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("get", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list_repos(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, ws_id: Option<String>| {
            // Unknown workspace / lock error → empty array, as the shell returned.
            let json = match host.workspace_list_repos(ws_id.as_deref()) {
                Ok(v) => v,
                Err(_) => return Ok(mlua::Value::Table(lua_ctx.create_table()?)),
            };
            lua_ctx.to_value(&json)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("list_repos", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_repo(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, repo_id: String| match host.workspace_repo(&repo_id) {
            Ok(Some(v)) => lua_ctx.to_value(&v),
            _ => Ok(mlua::Value::Nil),
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("repo", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_switch(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ws_table: &Table,
) -> PluginCoreResult<()> {
    // switch(ws_id) → (true, nil) | (false, err)
    //   Host marks the workspace active (persists), fires `corvus:workspace_switched`,
    //   and emits `arbor://workspace-switched` — same effects the shell produced.
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, ws_id: String| -> LuaTuple {
            match host.workspace_switch(&ws_id, &pname) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ws_table
        .set("switch", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
