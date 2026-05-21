//! `arbor.ci` — read-only git-provider CI access.
//!
//! Wraps `GitProvider::list_ci_runs` so plugins can poll CI state for any
//! registered repository without touching the OAuth token. Permission gate
//! is the same `provider = "read"` flag used by `arbor.mr.*`.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let ci_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_runs(ctx, lua, &ci_table)?;

    arbor.set("ci", ci_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

macro_rules! block_on_provider {
    ($fut:expr) => {{
        let rt = tokio::runtime::Handle::try_current().ok();
        if let Some(h) = rt {
            h.block_on($fut)
        } else {
            match tokio::runtime::Runtime::new() {
                Ok(r)  => r.block_on($fut),
                Err(e) => Err(crate::git_provider::types::error::ProviderError::Internal(
                    format!("runtime: {e}"),
                )),
            }
        }
    }};
}

fn install_runs(ctx: &ApiCtx, lua: &Lua, ci_table: &Table) -> Result<()> {
    // runs({ repo_id?, branch?, status?, mr_number?, per_page? })
    //   → ([CiRun], nil) | (nil, err)
    //
    // Filters are forwarded to the provider's `CiFilter`. Defaults: most
    // recent runs across all branches. Setting `branch` is the most common
    // use-case (the CI-failure-triage plugin scans MR head branches one by
    // one and asks for runs scoped to each).
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.ci.runs: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.ci.runs: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let repo_id: Option<String> = opts.get("repo_id").ok();
        let branch:  Option<String> = opts.get::<Option<String>>("branch").ok().flatten();
        let status:  Option<String> = opts.get::<Option<String>>("status").ok().flatten();
        let mr_number: Option<u64>  = opts.get::<Option<u64>>("mr_number").ok().flatten();
        let per_page:  Option<u32>  = opts.get::<Option<u32>>("per_page").ok().flatten();

        let path = match super::mr::resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.ci.runs: {e}")),
        };

        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.ci.runs resolve: {e}")),
        };

        let filter = crate::git_provider::types::CiFilter {
            branch,
            status,
            mr_number,
            page: None,
            per_page: per_page.or(Some(20)),
        };
        let runs = match block_on_provider!(
            resolved.provider.list_ci_runs(&resolved.repo, filter)
        ) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.ci.runs: {e}")),
        };

        let json = match serde_json::to_value(&runs) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.ci.runs encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("arbor.ci.runs to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ci_table.set("runs", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
