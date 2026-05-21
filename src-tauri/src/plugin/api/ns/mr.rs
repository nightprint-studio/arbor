//! `arbor.mr` — read-only git-provider MR / PR access.
//!
//! Plugins can list merge requests for any registered repository
//! WITHOUT ever seeing the OAuth token. The token lives in the OS
//! keyring; the host resolves it internally when calling
//! `GitProvider::list_mrs`. The plugin only sees the resulting
//! `MergeRequest` payloads.
//!
//! Calling convention:
//!   · `(value, nil)` on success, `(nil, err)` on recoverable failure.
//!   · Permission denied raises a Lua error.
//!   · `current_user(repo_id)` resolves the authenticated user on the
//!      provider for that repo — used as the implicit value for the
//!      `author = "current_user"` sentinel in `list`.

use mlua::{Lua, LuaSerdeExt, Table};
use tauri::Manager;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};

const CURRENT_USER_SENTINEL: &str = "current_user";

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let mr_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_list(ctx, lua, &mr_table)?;
    install_current_user(ctx, lua, &mr_table)?;

    arbor.set("mr", mr_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Drive an async call from a sync Lua context, reusing the current
/// tokio handle when one is available.
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

pub(super) fn resolve_repo_path(
    handle:  &tauri::AppHandle,
    repo_id: Option<&str>,
) -> std::result::Result<String, String> {
    // Two paths:
    //   · explicit repo_id from the workspace registry (the common case
    //     for plugins iterating across all registered repos)
    //   · implicit fallback to the active tab so simple "current repo"
    //     calls don't need to plumb the id around
    let state = handle.state::<crate::AppState>();
    if let Some(id) = repo_id {
        let reg = state.repo_registry.lock().map_err(|e| e.to_string())?;
        let entry = reg.get(id).ok_or_else(|| format!("repo '{id}' not registered"))?;
        return Ok(entry.path.clone());
    }
    let active = state.active_tab_id.lock()
        .map_err(|e| e.to_string())?
        .clone();
    let active = active.ok_or_else(|| "no active tab".to_string())?;
    let mut mgr = state.repos.lock().map_err(|e| e.to_string())?;
    let repo = mgr.get(&active).map_err(|e| format!("active tab not in manager: {e}"))?;
    let path = repo.inner().path().parent()
        .ok_or_else(|| "active repo has no parent path".to_string())?
        .to_string_lossy()
        .to_string();
    Ok(path)
}

fn install_list(ctx: &ApiCtx, lua: &Lua, mr_table: &Table) -> Result<()> {
    // list({ repo_id?, state?, author?, labels?, query? }) → ([mr], nil) | (nil, err)
    //
    //   repo_id : workspace registry id; defaults to the active repo
    //   state   : "open" | "closed" | "merged" | "all" (default "open")
    //   author  : login filter; pass "current_user" to mean "me on this provider"
    //   labels  : array of label names (post-filter; provider-side support varies)
    //   query   : free-text query forwarded to the provider's filter
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.mr.list: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.mr.list: app handle unavailable");
        };

        let opts = opts.unwrap_or_else(|| lua_ctx.create_table().unwrap());
        let repo_id: Option<String> = opts.get("repo_id").ok();
        let state_filter: String = opts.get::<Option<String>>("state").ok().flatten()
            .unwrap_or_else(|| "open".to_string());
        let mut author: Option<String> = opts.get::<Option<String>>("author").ok().flatten();
        let labels: Option<Vec<String>> = opts.get::<Option<Vec<String>>>("labels").ok().flatten();
        let query:  Option<String>      = opts.get::<Option<String>>("query").ok().flatten();

        let path = match resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.list: {e}")),
        };

        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.list resolve provider: {e}")),
        };

        // Resolve the "current_user" sentinel against the actual provider
        // so plugins never need to know (or store) the user's handle.
        // Auth failures here turn the filter into a no-op (None) rather
        // than poisoning the whole call: an un-authed user has no MRs
        // attributable to them anyway.
        if author.as_deref() == Some(CURRENT_USER_SENTINEL) {
            let user_res = block_on_provider!(resolved.provider.current_user());
            author = match user_res {
                Ok(u)  => Some(u.login),
                Err(_) => None,
            };
            if author.is_none() {
                return ok2(lua_ctx, lua_ctx.create_table()?);
            }
        }

        let filter = crate::git_provider::types::MrFilter {
            state:    Some(state_filter),
            author:   author.clone(),
            assignee: None,
            labels:   labels.clone(),
            query,
            page:     None,
            per_page: Some(100),
        };

        let mrs = match block_on_provider!(resolved.provider.list_mrs(&resolved.repo, filter)) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.list: {e}")),
        };

        // Some providers don't honor the `author` filter server-side
        // (or honor it as "involved-user" instead of "created-by"). Apply
        // a defensive client-side filter to keep the Lua-visible
        // semantics consistent across GitHub and GitLab.
        let mrs: Vec<_> = match author {
            Some(a) => mrs.into_iter()
                .filter(|m| m.author.login.eq_ignore_ascii_case(&a))
                .collect(),
            None => mrs,
        };

        let json = match serde_json::to_value(&mrs) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.list encode: {e}")),
        };
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("arbor.mr.list to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    mr_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_current_user(ctx: &ApiCtx, lua: &Lua, mr_table: &Table) -> Result<()> {
    // current_user({repo_id?}) → ({login, name?, ...}, nil) | (nil, err)
    // Resolves the authenticated user on the provider attached to the
    // given repo. Plugins can use this to display "you" in their UI
    // without ever touching the token.
    let provider_read = ctx.provider_read;
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, opts: Option<Table>| -> LuaTuple {
        if !provider_read {
            return Err(mlua::Error::RuntimeError(
                "arbor.mr.current_user: requires provider = \"read\" (or higher)".to_string()
            ));
        }
        let Some(ref h) = handle else {
            return err2(lua_ctx, "arbor.mr.current_user: app handle unavailable");
        };
        let repo_id: Option<String> = opts.and_then(|t| t.get("repo_id").ok());
        let path = match resolve_repo_path(h, repo_id.as_deref()) {
            Ok(p)  => p,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.current_user: {e}")),
        };
        let state_app = h.state::<crate::AppState>();
        let resolved = match crate::git_provider::provider_for_path(&state_app, &path) {
            Ok(r)  => r,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.current_user resolve: {e}")),
        };
        let user = match block_on_provider!(resolved.provider.current_user()) {
            Ok(u)  => u,
            Err(e) => return err2(lua_ctx, format!("arbor.mr.current_user: {e}")),
        };
        let json = serde_json::to_value(&user)
            .map_err(|e| mlua::Error::RuntimeError(format!("encode: {e}")))?;
        match lua_ctx.to_value(&json) {
            Ok(v)  => ok2(lua_ctx, v),
            Err(e) => err2(lua_ctx, format!("to_value: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    mr_table.set("current_user", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
