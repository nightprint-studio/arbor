//! `arbor.mr` (read-only git-provider MR / PR access), ported to run through an
//! [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's `ns_shell/mr.rs`:
//! same namespace (`arbor.mr`), same function names (`list` / `current_user`),
//! same argument shapes (table-config opts with `repo_id` / `state` / `author` /
//! `labels` / `query`), same `(value, err)` tuple conventions, same permission-gate
//! `RuntimeError` strings, same `arbor.mr.<op>: …` error prefixes, and the same
//! `"current_user"` sentinel for the `author` filter. The only difference is
//! *where the work goes*:
//!
//!   · the shell resolved the repo path via `resolve_repo_path` (explicit
//!     `repo_id` from the workspace registry, else the active tab), resolved the
//!     provider with `git_provider::provider_for_path`, and `block_on`-drove the
//!     provider's async REST calls inline;
//!   · here the active repo is the `__arbor_current_repo__` Lua global (the same
//!     active-repo path `arbor.repo.*` reads), and the provider work goes through
//!     the captured `Arc<dyn NsHost>`, which resolves `repo_id` against the
//!     `corvus-be` workspace registry (else falls back to the active path),
//!     resolves the provider over the reverse channel (`be::provider`), and blocks
//!     on the corvus-be runtime exactly as `be/src/mr.rs` does — so results and
//!     `ProviderError` wire strings match.
//!
//! Plugins never see the OAuth token: the host resolves it internally when calling
//! `GitProvider::list_mrs` / `current_user`. Only the resulting payloads cross.
//!
//! Calling convention (unchanged):
//!   · `list({repo_id?, state?, author?, labels?, query?})` → `([mr], nil) | (nil, err)`.
//!   · `current_user({repo_id?})` → `({login, …}, nil) | (nil, err)`.
//! Requires provider ≥ Read for both.

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `author = "current_user"` sentinel — resolved by the host against the actual
/// provider so plugins never need to know (or store) the user's handle.
const CURRENT_USER_SENTINEL: &str = "current_user";

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active — the host's `repo_id` fallback then surfaces the same
/// `"no active tab"` error the shell's `resolve_repo_path` produced.
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.mr.*` installer. Holds the host handle the closures call through.
pub struct MrInstaller {
    host: NsHostHandle,
}

impl MrInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for MrInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let mr_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_list(self.host.clone(), ctx, lua, &mr_table)?;
        install_current_user(self.host.clone(), ctx, lua, &mr_table)?;

        arbor
            .set("mr", mr_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_list(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    mr_table: &Table,
) -> PluginCoreResult<()> {
    // list({ repo_id?, state?, author?, labels?, query? }) → ([mr], nil) | (nil, err)
    //
    //   repo_id : workspace registry id; defaults to the active repo
    //   state   : "open" | "closed" | "merged" | "all" (default "open")
    //   author  : login filter; pass "current_user" to mean "me on this provider"
    //   labels  : array of label names (post-filter; provider-side support varies)
    //   query   : free-text query forwarded to the provider's filter
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.mr.list: requires provider = \"read\" (or higher)".to_string(),
                ));
            }

            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let repo_id: Option<String> = opts.get("repo_id").ok();
            let state_filter: String = opts
                .get::<Option<String>>("state")
                .ok()
                .flatten()
                .unwrap_or_else(|| "open".to_string());
            let author: Option<String> = opts.get::<Option<String>>("author").ok().flatten();
            let labels: Option<Vec<String>> =
                opts.get::<Option<Vec<String>>>("labels").ok().flatten();
            let query: Option<String> = opts.get::<Option<String>>("query").ok().flatten();

            // The `"current_user"` sentinel resolves to "me on this provider"
            // host-side: on a successful resolve it becomes the login; on auth
            // failure it becomes a no-op (empty result), never poisoning the call
            // — matching the shell's early `ok2(empty)`.
            let resolve_current_user = author.as_deref() == Some(CURRENT_USER_SENTINEL);

            let active = current_repo(lua_ctx);
            let json = match host.mr_list(
                active.as_deref(),
                repo_id.as_deref(),
                &state_filter,
                author.as_deref(),
                resolve_current_user,
                labels.as_deref(),
                query.as_deref(),
            ) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("arbor.mr.list to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    mr_table
        .set("list", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_current_user(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    mr_table: &Table,
) -> PluginCoreResult<()> {
    // current_user({repo_id?}) → ({login, name?, ...}, nil) | (nil, err)
    // Resolves the authenticated user on the provider attached to the given repo.
    // Plugins use this to display "you" in their UI without touching the token.
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.mr.current_user: requires provider = \"read\" (or higher)".to_string(),
                ));
            }
            let repo_id: Option<String> = opts.and_then(|t| t.get("repo_id").ok());
            let active = current_repo(lua_ctx);
            let json = match host.mr_current_user(active.as_deref(), repo_id.as_deref()) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };
            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    mr_table
        .set("current_user", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
