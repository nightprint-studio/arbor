//! `arbor.tabs` — programmatic tab control.
//!
//! Surface kept minimal: plugins can request that a registered repo be
//! brought into focus as a tab. The host owns the actual tab state, so
//! the Lua call resolves the `repo_id` against the registry and emits
//! `arbor://open-repo-tab { repo_id, path, display_name, remote_url? }`.
//! The frontend's AppShell listens and runs the same flow as
//! `WorkspaceManagementModal.openRepoTab` (ensure-registered → activate
//! existing tab or open a new one).
//!
//! Calling convention: `(true, nil)` on success, `(false, err)` on
//! recoverable failure (unknown repo_id, no app handle, …).

use mlua::{Lua, Table};
use tauri::{Emitter, Manager};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::tuple::{LuaTuple, boolerr2};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let tabs_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_open_repo(ctx, lua, &tabs_table)?;

    arbor.set("tabs", tabs_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_open_repo(ctx: &ApiCtx, lua: &Lua, tabs: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, repo_id: String| -> LuaTuple {
        let Some(ref h) = handle else {
            return boolerr2(lua_ctx, false, Some("app handle unavailable".into()));
        };
        let state = h.state::<crate::AppState>();
        let entry = {
            let reg = match state.repo_registry.lock() {
                Ok(r)  => r,
                Err(e) => return boolerr2(lua_ctx, false, Some(format!("registry lock: {e}"))),
            };
            match reg.get(&repo_id) {
                Some(e) => e.clone(),
                None    => return boolerr2(lua_ctx, false, Some(format!("repo '{repo_id}' not in registry"))),
            }
        };
        let payload = serde_json::json!({
            "repo_id":      entry.id,
            "path":         entry.path,
            "display_name": entry.display_name,
            "remote_url":   entry.remote_url,
        });
        let _ = h.emit("arbor://open-repo-tab", &payload);
        boolerr2(lua_ctx, true, None)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    tabs.set("open_repo", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
