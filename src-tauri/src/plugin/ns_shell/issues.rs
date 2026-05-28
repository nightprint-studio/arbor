//! `arbor.issues` (Linear integration — synchronous wrappers).
//!
//! Calling convention (Phase 1+2):
//!   · search/get/transition/comment perform network I/O. They return
//!     `(value, nil)` / `(nil, err)`.
//!   · branch_name(issue) is pure compute — keeps single-value return.
//!     A malformed issue table is a programming error → raise.

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let issues_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_search(ctx, lua, &issues_table)?;
    install_get(ctx, lua, &issues_table)?;
    install_lookup(ctx, lua, &issues_table)?;
    install_transition(ctx, lua, &issues_table)?;
    install_comment(ctx, lua, &issues_table)?;
    install_branch_name(lua, &issues_table)?;

    arbor.set("issues", issues_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Drive an async call from Lua by either reusing the current tokio
/// handle or spinning up a one-shot runtime. Returns Result<T, String>.
macro_rules! block_on_linear {
    ($fut:expr) => {{
        let rt = tokio::runtime::Handle::try_current().ok();
        let r = if let Some(h) = rt {
            h.block_on($fut)
        } else {
            match tokio::runtime::Runtime::new() {
                Ok(r) => r.block_on($fut),
                Err(e) => Err(crate::error::AppError::Other(format!("runtime: {e}"))),
            }
        };
        r.map_err(|e: crate::error::AppError| e.to_string())
    }};
}

fn install_search(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua.create_function(move |lua_ctx, filters: Option<mlua::Table>| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.issues.search: requires issues = \"read\" (or higher)".to_string()
            ));
        }
        let f: crate::integrations::IssueFilters = filters
            .map(|t| lua_ctx.from_value(mlua::Value::Table(t)).unwrap_or_default())
            .unwrap_or_default();
        let issues = match block_on_linear!(crate::integrations::linear::search_issues(f)) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.search: {e}")),
        };
        let json = match serde_json::to_value(&issues) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.search encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("issues.search to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("search", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua.create_function(move |lua_ctx, id: String| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.issues.get: requires issues = \"read\" (or higher)".to_string()
            ));
        }
        let issue = match block_on_linear!(crate::integrations::linear::get_issue(&id)) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.get: {e}")),
        };
        let json = match serde_json::to_value(&issue) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.get encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("issues.get to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("get", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_transition(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let write = ctx.issues_write;
    let fn_ = lua.create_function(move |lua_ctx, (id, status_id): (String, String)| -> LuaTuple {
        if !write {
            return Err(mlua::Error::RuntimeError(
                "arbor.issues.transition: requires issues = \"write\"".to_string()
            ));
        }
        let issue = match block_on_linear!(
            crate::integrations::linear::transition_issue(&id, &status_id)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.transition: {e}")),
        };
        let json = match serde_json::to_value(&issue) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.transition encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("issues.transition to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("transition", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_comment(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    let write = ctx.issues_write;
    let fn_ = lua.create_function(move |lua_ctx, (issue_id, body): (String, String)| -> LuaTuple {
        if !write {
            return Err(mlua::Error::RuntimeError(
                "arbor.issues.comment: requires issues = \"write\"".to_string()
            ));
        }
        let comment = match block_on_linear!(
            crate::integrations::linear::add_comment(&issue_id, &body)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.comment: {e}")),
        };
        let json = match serde_json::to_value(&comment) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.comment encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("issues.comment to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("comment", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_lookup(ctx: &ApiCtx, lua: &Lua, t: &Table) -> Result<()> {
    // lookup(identifier) → (issue|nil, nil) | (nil, err)
    //
    // Resolves a single issue by its human identifier (e.g. "ENG-42",
    // "PROJ-123") against the tracker configured for the active repo
    // (`repo_config.issue_tracker`). Cross-tracker by design: each
    // workspace project can be bound to its own tracker, and this
    // function routes per repo without the plugin having to care.
    //
    // Returns the issue table on hit, nil on miss (no tracker / no
    // match), and (nil, err) on auth / network failure.
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua.create_function(move |lua_ctx, identifier: String| -> LuaTuple {
        if !read {
            return Err(mlua::Error::RuntimeError(
                "arbor.issues.lookup: requires issues = \"read\" (or higher)".to_string()
            ));
        }
        // Resolve the active repo from the per-plugin Lua global. No
        // repo → no tracker → no lookup; mirror what `arbor.repo.current`
        // would return (nil) so the caller can render the bare key.
        let Some(repo_path) = lua_ctx.globals()
            .get::<Option<String>>("__arbor_current_repo__").unwrap_or(None)
        else { return ok2(lua_ctx, mlua::Value::Nil); };

        let issue_opt = match block_on_linear!(
            crate::integrations::lookup_by_identifier(&repo_path, &identifier)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.lookup: {e}")),
        };
        let Some(issue) = issue_opt else {
            return ok2(lua_ctx, mlua::Value::Nil);
        };
        let json = match serde_json::to_value(&issue) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("issues.lookup encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("issues.lookup to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("lookup", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branch_name(lua: &Lua, t: &Table) -> Result<()> {
    // branch_name(issue) → string  (pure compute — bad input raises)
    let fn_ = lua.create_function(move |lua_ctx, issue: mlua::Table| {
        let json: serde_json::Value = lua_ctx
            .from_value(mlua::Value::Table(issue))
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        let i: crate::integrations::Issue = serde_json::from_value(json)
            .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
        Ok(crate::integrations::branch_name_for_issue(&i))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    t.set("branch_name", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
