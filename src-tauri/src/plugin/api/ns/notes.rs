//! `arbor.notes` (git notes — read/write notes attached to commits).
//!
//! Calling convention (Phase 1+2):
//!   · `list(oid)` and `get(oid, namespace)` are 1-2 mandatory args, kept
//!     positional. They return `(value, nil) | (nil, err)`.
//!   · `set{oid, namespace, content}` is 3 args → table-config.
//!   · `delete(oid, namespace)` stays positional, returns tuple.
//! Requires git ≥ Read for list/get, git ≥ Write for set/delete.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let notes_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(ctx, lua, &notes_table)?;
    install_get(ctx, lua, &notes_table)?;
    install_set(ctx, lua, &notes_table)?;
    install_delete(ctx, lua, &notes_table)?;

    arbor.set("notes", notes_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, notes_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, commit_oid: String| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.notes.list: requires git = \"read\" (or higher)".to_string(),
            ));
        }
        let Some(ref h) = handle else {
            return ok2(lua_ctx, lua_ctx.create_table()?);
        };
        let state = h.state::<crate::AppState>();
        let tab_id = state.active_tab_id.lock()
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
            .clone()
            .unwrap_or_default();
        if tab_id.is_empty() {
            return ok2(lua_ctx, lua_ctx.create_table()?);
        }
        let mut mgr = match state.lock_repos() {
            Ok(g)  => g,
            Err(e) => return err2(lua_ctx, format!("notes.list repos lock: {e}")),
        };
        let repo = match mgr.get(&tab_id) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("notes.list repo: {e}")),
        };
        let notes = match crate::git::notes::list_notes(repo.inner(), &commit_oid) {
            Ok(n)  => n,
            Err(e) => return err2(lua_ctx, format!("notes.list: {e}")),
        };
        let json = match serde_json::to_value(&notes) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("notes.list encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("notes.list to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    notes_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, notes_table: &Table) -> Result<()> {
    let git_read = ctx.git_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, (commit_oid, namespace): (String, String)| -> LuaTuple {
        if !git_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.notes.get: requires git = \"read\" (or higher)".to_string(),
            ));
        }
        let Some(ref h) = handle else { return ok2(lua_ctx, mlua::Value::Nil); };
        let state  = h.state::<crate::AppState>();
        let tab_id = {
            let guard = state.active_tab_id.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            guard.clone().unwrap_or_default()
        };
        if tab_id.is_empty() { return ok2(lua_ctx, mlua::Value::Nil); }
        let result: std::result::Result<Option<String>, String> = (|| {
            let mut mgr  = state.lock_repos().map_err(|e| e.to_string())?;
            let repo = mgr.get(&tab_id).map_err(|e| e.to_string())?;
            let oid  = git2::Oid::from_str(&commit_oid).map_err(|e| e.to_string())?;
            let notes_ref = format!("refs/notes/{namespace}");
            let content = match repo.inner().find_note(Some(&notes_ref), oid) {
                Ok(note) => Some(note.message().unwrap_or("").to_string()),
                Err(_)   => None,
            };
            Ok(content)
        })();
        match result {
            Ok(Some(s)) => ok2(lua_ctx, s),
            Ok(None)    => ok2(lua_ctx, mlua::Value::Nil),
            Err(e)      => err2(lua_ctx, format!("notes.get: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    notes_table.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set(ctx: &ApiCtx, lua: &Lua, notes_table: &Table) -> Result<()> {
    let git_write = ctx.git_write;
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        if !git_write {
            return Err(mlua::Error::RuntimeError(
                "arbor.notes.set: requires git = \"write\" (or higher)".to_string(),
            ));
        }
        let commit_oid: String = cfg.get("commit_oid").map_err(|_|
            mlua::Error::RuntimeError("arbor.notes.set: 'commit_oid' is required".into()))?;
        let namespace: String = cfg.get("namespace").map_err(|_|
            mlua::Error::RuntimeError("arbor.notes.set: 'namespace' is required".into()))?;
        let content: String = cfg.get("content").map_err(|_|
            mlua::Error::RuntimeError("arbor.notes.set: 'content' is required".into()))?;

        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("notes.set: app handle unavailable".into()));
        };
        let state = h.state::<crate::AppState>();
        let tab_id = {
            let guard = state.active_tab_id.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            guard.clone().unwrap_or_default()
        };
        if tab_id.is_empty() {
            return boolerr2(lua_ctx, false, Some("notes.set: no active tab".into()));
        }
        let res: std::result::Result<(), String> = (|| {
            let mut mgr  = state.lock_repos().map_err(|e| e.to_string())?;
            let repo = mgr.get(&tab_id).map_err(|e| e.to_string())?;
            crate::git::notes::set_note(repo.inner(), &commit_oid, &namespace, &content)
                .map_err(|e| e.to_string())
        })();
        if let Err(e) = res {
            return boolerr2(lua_ctx, false, Some(format!("notes.set: {e}")));
        }
        if let Ok(host) = state.lock_plugin_host() {
            let ctx_json = serde_json::json!({
                "tab_id": &tab_id, "commit_oid": &commit_oid,
                "namespace": &namespace, "plugin": &pname,
            });
            let _ = host.fire_hook("on_note_saved", &ctx_json.to_string());
        }
        boolerr2(lua_ctx, true, None)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    notes_table.set("set", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_delete(ctx: &ApiCtx, lua: &Lua, notes_table: &Table) -> Result<()> {
    let git_write = ctx.git_write;
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |lua_ctx, (commit_oid, namespace): (String, String)| -> LuaTuple {
        if !git_write {
            return Err(mlua::Error::RuntimeError(
                "arbor.notes.delete: requires git = \"write\" (or higher)".to_string(),
            ));
        }
        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("notes.delete: app handle unavailable".into()));
        };
        let state = h.state::<crate::AppState>();
        let tab_id = {
            let guard = state.active_tab_id.lock()
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            guard.clone().unwrap_or_default()
        };
        if tab_id.is_empty() {
            return boolerr2(lua_ctx, false, Some("notes.delete: no active tab".into()));
        }
        let res: std::result::Result<(), String> = (|| {
            let mut mgr  = state.lock_repos().map_err(|e| e.to_string())?;
            let repo = mgr.get(&tab_id).map_err(|e| e.to_string())?;
            crate::git::notes::delete_note(repo.inner(), &commit_oid, &namespace)
                .map_err(|e| e.to_string())
        })();
        if let Err(e) = res {
            return boolerr2(lua_ctx, false, Some(format!("notes.delete: {e}")));
        }
        if let Ok(host) = state.lock_plugin_host() {
            let ctx_json = serde_json::json!({
                "tab_id": &tab_id, "commit_oid": &commit_oid,
                "namespace": &namespace, "plugin": &pname,
            });
            let _ = host.fire_hook("on_note_deleted", &ctx_json.to_string());
        }
        boolerr2(lua_ctx, true, None)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    notes_table.set("delete", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
