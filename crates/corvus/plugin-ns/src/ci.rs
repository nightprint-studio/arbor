//! `arbor.ci` (read-only git-provider CI access), ported to run through an
//! [`NsHost`] instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's `ns_shell/ci.rs`:
//! same namespace (`arbor.ci`), same function name (`runs`), same option-table
//! shape (`{ repo_id?, branch?, status?, mr_number?, per_page? }`), same
//! `(value, err)` tuple convention, same permission-gate `RuntimeError` string,
//! and the same `arbor.ci.runs[ …]: …` error prefixes. The only difference is
//! *where the work goes*:
//!
//!   · the shell resolved the repo via `resolve_repo_path(handle, repo_id)`
//!     (explicit `repo_id` → workspace registry path, else the active tab),
//!     resolved the provider with `git_provider::provider_for_path`, and drove
//!     the async `GitProvider::list_ci_runs` on the in-process tokio handle;
//!   · here the active repo is the `__arbor_current_repo__` Lua global (the same
//!     active-repo path `arbor.repo.*` reads), an optional `repo_id` is forwarded
//!     verbatim, and all of that work — repo resolution, provider lookup, the
//!     async list call, and the serde encode — goes through the captured
//!     `Arc<dyn NsHost>`, which uses the corvus-be provider registry +
//!     `corvus-git-provider-*` (the same trait impls the shell uses), so results
//!     and `ProviderError` wire strings match.
//!
//! No hooks fire in this namespace (read-only), exactly as in the shell.
//!
//! Calling convention (unchanged):
//!   · `runs({ repo_id?, branch?, status?, mr_number?, per_page? })`
//!     → `([CiRun], nil) | (nil, err)`. The option table is optional; an absent
//!     table behaves like an empty one (most-recent runs across all branches).
//! Requires `provider = "read"` (or higher).

use mlua::{Lua, LuaSerdeExt, Table};

use arbor_plugin_core::prelude::{
    err2, ok2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// Read the active repo path from the `__arbor_current_repo__` Lua global. `None`
/// when no repo is active; the host then falls back to `repo_id` (and errors with
/// the shell's `resolve_repo_path` wire string when neither is available).
fn current_repo(lua: &Lua) -> Option<String> {
    lua.globals()
        .get::<Option<String>>("__arbor_current_repo__")
        .unwrap_or(None)
}

/// `arbor.ci.*` installer. Holds the host handle the closures call through.
pub struct CiInstaller {
    host: NsHostHandle,
}

impl CiInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for CiInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let ci_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_runs(self.host.clone(), ctx, lua, &ci_table)?;

        arbor
            .set("ci", ci_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_runs(
    host: NsHostHandle,
    ctx: &ApiCtx,
    lua: &Lua,
    ci_table: &Table,
) -> PluginCoreResult<()> {
    // runs({ repo_id?, branch?, status?, mr_number?, per_page? })
    //   → ([CiRun], nil) | (nil, err)
    //
    // Filters are forwarded to the provider's `CiFilter`. Defaults: most recent
    // runs across all branches. Setting `branch` is the most common use-case (the
    // CI-failure-triage plugin scans MR head branches one by one and asks for runs
    // scoped to each).
    let provider_read = ctx.provider_read;
    let fn_ = lua
        .create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
            if !provider_read {
                return Err(mlua::Error::RuntimeError(
                    "arbor.ci.runs: requires provider = \"read\" (or higher)".to_string(),
                ));
            }

            let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
            let repo_id: Option<String> = opts.get("repo_id").ok();
            let branch: Option<String> = opts.get::<Option<String>>("branch").ok().flatten();
            let status: Option<String> = opts.get::<Option<String>>("status").ok().flatten();
            let mr_number: Option<u64> = opts.get::<Option<u64>>("mr_number").ok().flatten();
            let per_page: Option<u32> = opts.get::<Option<u32>>("per_page").ok().flatten();

            // Active repo path from the Lua global; the host falls back to
            // `repo_id` and errors with the shell's resolve wire string when
            // neither resolves a repo.
            let repo_path = current_repo(lua_ctx);

            // Host does: resolve repo (path | repo_id), resolve provider, build
            // `CiFilter`, run the async `list_ci_runs`, and serde-encode the
            // result — applying the same `arbor.ci.runs[ resolve| encode]: …`
            // prefixes the shell applied host-side.
            let json = match host.ci_runs(
                repo_path.as_deref(),
                repo_id.as_deref(),
                branch.as_deref(),
                status.as_deref(),
                mr_number,
                per_page,
            ) {
                Ok(v) => v,
                Err(e) => return err2(lua_ctx, e),
            };

            match lua_ctx.to_value(&json) {
                Ok(v) => ok2(lua_ctx, v),
                Err(e) => err2(lua_ctx, format!("arbor.ci.runs to_value: {e}")),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    ci_table
        .set("runs", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
