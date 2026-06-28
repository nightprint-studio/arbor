//! `arbor.issues` (issue-tracker integration — synchronous wrappers), ported to
//! run through an [`NsHost`] instead of the shell's `crate::integrations::*`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/issues.rs`: same namespace (`arbor.issues`), same function names
//! (`search` / `get` / `lookup` / `transition` / `comment` / `branch_name`), same
//! argument shapes, same `(value, err)` tuple conventions, same permission-gate
//! `RuntimeError` strings, same `issues.<op>: …` error prefixes. The only
//! difference is *where the work goes*:
//!
//!   · the shell drove `crate::integrations::linear::*` (Linear-only) and the
//!     per-repo `crate::integrations::lookup_by_identifier` directly, blocking on
//!     a tokio handle via its `block_on_linear!` macro;
//!   · here every network call goes through the captured `Arc<dyn NsHost>`, which
//!     in `corvus-be` reaches the reverse-channel-backed tracker registry and
//!     blocks on the backend's own runtime — so results and error text match.
//!
//! Calling convention (unchanged):
//!   · `search/get/transition/comment/lookup` perform network I/O. They return
//!     `(value, nil)` / `(nil, err)`. `lookup` additionally returns `(nil, nil)`
//!     when there is no active repo / no tracker / no match.
//!   · `branch_name(issue)` is pure compute — keeps a single-value return. A
//!     malformed issue table is a programming error → raise.
//! Requires issues ≥ Read for search/get/lookup, issues ≥ Write for
//! transition/comment.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active (mirrors the shell's "no repo → no tracker → nil" path
/// in `lookup`).
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.issues.*` installer. Holds the host handle the closures call through.
pub struct IssuesInstaller {
    host: NsHostHandle,
}

impl IssuesInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for IssuesInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let issues_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_search(self.host.clone(), ctx, lua, &issues_table)?;
        install_get(self.host.clone(), ctx, lua, &issues_table)?;
        install_lookup(self.host.clone(), ctx, lua, &issues_table)?;
        install_transition(self.host.clone(), ctx, lua, &issues_table)?;
        install_comment(self.host.clone(), ctx, lua, &issues_table)?;
        install_branch_name(self.host.clone(), lua, &issues_table)?;

        arbor
            .set("issues", issues_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_search(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua
        .create_function(move |lua_ctx, filters: Option<mlua::Table>| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.issues.search: requires issues = \"read\" (or higher)".to_string(),
                ));
            }
            // Marshal the optional filters table to JSON; a malformed table
            // degrades to `null` (the host treats it as `IssueFilters::default`),
            // matching the shell's `unwrap_or_default()`.
            let filters_json = match filters {
                Some(tbl) => lua_ctx
                    .from_value::<serde_json::Value>(mlua::Value::Table(tbl))
                    .unwrap_or(serde_json::Value::Null),
                None => serde_json::Value::Null,
            };
            let json = match host.issues_search(filters_json) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("issues.search to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("search", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_get(host: NsHostHandle, ctx: &ApiCtx, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua
        .create_function(move |lua_ctx, id: String| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.issues.get: requires issues = \"read\" (or higher)".to_string(),
                ));
            }
            let json = match host.issues_get(&id) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("issues.get to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("get", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_lookup(host: NsHostHandle, ctx: &ApiCtx, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // lookup(identifier) → (issue|nil, nil) | (nil, err)
    //
    // Resolves a single issue by its human identifier (e.g. "ENG-42",
    // "PROJ-123") against the tracker configured for the active repo
    // (`repo_config.issue_tracker`). Cross-tracker by design: each workspace
    // project can be bound to its own tracker, and this function routes per repo
    // without the plugin having to care.
    //
    // Returns the issue table on hit, nil on miss (no tracker / no match), and
    // (nil, err) on auth / network failure.
    let read = ctx.issues_read || ctx.issues_write;
    let fn_ = lua
        .create_function(move |lua_ctx, identifier: String| -> LuaTuple {
            if !read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.issues.lookup: requires issues = \"read\" (or higher)".to_string(),
                ));
            }
            // Resolve the active repo from the per-plugin Lua global. No repo →
            // no tracker → no lookup; mirror what `arbor.repo.current` would
            // return (nil) so the caller can render the bare key.
            let Some(repo_path) = current_repo(lua_ctx) else {
                return ok2(lua_ctx, mlua::Value::Nil);
            };
            let issue_opt = match host.issues_lookup(&repo_path, &identifier) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            let Some(json) = issue_opt else {
                return ok2(lua_ctx, mlua::Value::Nil);
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("issues.lookup to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("lookup", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_transition(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let write = ctx.issues_write;
    let fn_ = lua
        .create_function(
            move |lua_ctx, (id, status_id): (String, String)| -> LuaTuple {
                if !write {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.issues.transition: requires issues = \"write\"".to_string(),
                    ));
                }
                let json = match host.issues_transition(&id, &status_id) {
                    Ok(v) => v,
                    Err(e) => return err2(lua_ctx, e),
                };
                match lua_ctx.to_value(&json) {
                    Ok(v) => ok2(lua_ctx, v),
                    Err(e) => err2(lua_ctx, format!("issues.transition to_value: {e}")),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("transition", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_comment(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    t: &Table,
) -> PluginCoreResult<()> {
    let write = ctx.issues_write;
    let fn_ = lua
        .create_function(
            move |lua_ctx, (issue_id, body): (String, String)| -> LuaTuple {
                if !write {
                    return Err(mlua::Error::RuntimeError(
                        "arbor.issues.comment: requires issues = \"write\"".to_string(),
                    ));
                }
                let json = match host.issues_comment(&issue_id, &body) {
                    Ok(v) => v,
                    Err(e) => return err2(lua_ctx, e),
                };
                match lua_ctx.to_value(&json) {
                    Ok(v) => ok2(lua_ctx, v),
                    Err(e) => err2(lua_ctx, format!("issues.comment to_value: {e}")),
                }
            },
        )
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("comment", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_branch_name(host: NsHostHandle, lua: &Lua, t: &Table) -> PluginCoreResult<()> {
    // branch_name(issue) → string  (pure compute — bad input raises)
    //
    // The slugify lives in `corvus-issue-tracker-api::branch_name_for_issue`,
    // which this light crate does not depend on, so the issue table crosses to
    // the host as JSON and the host deserializes + slugifies. A malformed table
    // surfaces as a hard `RuntimeError` (the host returns `Err`), matching the
    // shell's raise-on-bad-shape behaviour.
    let fn_ = lua
        .create_function(move |lua_ctx, issue: mlua::Table| {
            let json: serde_json::Value = lua_ctx
                .from_value(mlua::Value::Table(issue))
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            host.issues_branch_name(json)
                .map_err(mlua::Error::RuntimeError)
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    t.set("branch_name", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
