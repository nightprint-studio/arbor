//! `arbor.notes` (git notes — read/write notes attached to commits), ported to
//! run through an [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/notes.rs`: same namespace (`arbor.notes`), same function names
//! (`list` / `get` / `set` / `delete`), same argument shapes, same `(value, err)`
//! tuple conventions, same permission-gate `RuntimeError` strings, same
//! `notes.<op>: …` error prefixes. The only difference is *where the work goes*:
//!
//!   · the shell resolved the active repo via `AppState::active_tab_id` +
//!     `lock_repos()` and called `crate::git::notes::*` directly;
//!   · here the active repo is the `__arbor_current_repo__` Lua global (the same
//!     active-repo path `arbor.repo.*` reads), and the git work goes through the
//!     captured `Arc<dyn NsHost>`, which opens that path with git2 and runs the
//!     shared `corvus-git::notes` logic — so results and error text match.
//!
//! Hooks (`on_note_saved` / `on_note_deleted`) are fired by the host inside
//! `notes_set` / `notes_delete` (the host owns the plugin host), exactly as the
//! shell fired them inline after the write.
//!
//! Calling convention (unchanged):
//!   · `list(oid)` and `get(oid, namespace)` are positional, returning
//!     `(value, nil) | (nil, err)`.
//!   · `set{commit_oid, namespace, content}` is a table-config (3 args).
//!   · `delete(oid, namespace)` is positional, returning the bool tuple.
//! Requires git ≥ Read for list/get, git ≥ Write for set/delete.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    boolerr2, err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError,
    PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active (mirrors the shell's "empty tab → empty result" path).
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.notes.*` installer. Holds the host handle the closures call through.
pub struct NotesInstaller {
    host: NsHostHandle,
}

impl NotesInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for NotesInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let notes_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_list(self.host.clone(), ctx, lua, &notes_table)?;
        install_get(self.host.clone(), ctx, lua, &notes_table)?;
        install_set(self.host.clone(), ctx, lua, &notes_table)?;
        install_delete(self.host.clone(), ctx, lua, &notes_table)?;

        arbor
            .set("notes", notes_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_list(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    notes_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(move |lua_ctx, commit_oid: String| -> LuaTuple {
            if !git_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.notes.list: requires git = \"read\" (or higher)".to_string(),
                ));
            }
            // No active repo → empty list (mirrors the shell's empty-tab path).
            let Some(repo_path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            };
            let json = match host.notes_list(&repo_path, &commit_oid) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("notes.list to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    notes_table
        .set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    notes_table: &Table,
) -> PluginCoreResult<()> {
    let git_read = ctx.git_read;
    let fn_ = lua
        .create_function(
            move |lua_ctx, (commit_oid, namespace): (String, String)| -> LuaTuple {
                if !git_read {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.notes.get: requires git = \"read\" (or higher)".to_string(),
                    ));
                }
                let Some(repo_path) = current_repo(lua_ctx) else {
                    return ok2(lua_ctx, mlua::Value::Nil);
                };
                match host.notes_get(&repo_path, &commit_oid, &namespace) {
                    Ok(Some(s)) => ok2(lua_ctx, s),
                    Ok(None) => ok2(lua_ctx, mlua::Value::Nil),
                    Err(e) => err2(lua_ctx, e),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    notes_table
        .set("get", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    notes_table: &Table,
) -> PluginCoreResult<()> {
    let git_write = ctx.git_write;
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            if !git_write {
                return Err(mlua::Error::RuntimeError(
                    "arbor.notes.set: requires git = \"write\" (or higher)".to_string(),
                ));
            }
            let commit_oid: String = cfg.get("commit_oid").map_err(|_| {
                mlua::Error::RuntimeError("arbor.notes.set: 'commit_oid' is required".into())
            })?;
            let namespace: String = cfg.get("namespace").map_err(|_| {
                mlua::Error::RuntimeError("arbor.notes.set: 'namespace' is required".into())
            })?;
            let content: String = cfg.get("content").map_err(|_| {
                mlua::Error::RuntimeError("arbor.notes.set: 'content' is required".into())
            })?;

            let Some(repo_path) = current_repo(lua_ctx) else {
                return boolerr2(lua_ctx, false, Some("notes.set: no active tab".into()));
            };
            match host.notes_set(&repo_path, &commit_oid, &namespace, &content, &pname) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(format!("notes.set: {e}"))),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    notes_table
        .set("set", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_delete(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    notes_table: &Table,
) -> PluginCoreResult<()> {
    let git_write = ctx.git_write;
    let pname = ctx.plugin_name.clone();
    let fn_ = lua
        .create_function(
            move |lua_ctx, (commit_oid, namespace): (String, String)| -> LuaTuple {
                if !git_write {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.notes.delete: requires git = \"write\" (or higher)".to_string(),
                    ));
                }
                let Some(repo_path) = current_repo(lua_ctx) else {
                    return boolerr2(lua_ctx, false, Some("notes.delete: no active tab".into()));
                };
                match host.notes_delete(&repo_path, &commit_oid, &namespace, &pname) {
                    Ok(()) => boolerr2(lua_ctx, true, None),
                    Err(e) => boolerr2(lua_ctx, false, Some(format!("notes.delete: {e}"))),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    notes_table
        .set("delete", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
