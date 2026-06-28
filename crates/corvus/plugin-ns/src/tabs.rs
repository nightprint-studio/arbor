//! `arbor.tabs` — programmatic tab control, ported to run through an [`NsHost`]
//! instead of a `tauri::AppState`.
//!
//! Lua-visible surface is **byte-for-byte** that of the shell's
//! `ns_shell/tabs.rs`: same namespace (`arbor.tabs`), same function name
//! (`open_repo`), same positional `String` argument (`repo_id`), same
//! `(true, nil) | (false, err)` tuple convention, same error strings.
//!
//! DIRECT namespace: unlike the proxy namespaces, the work lives in
//! `corvus-be` itself. The shell resolved `repo_id` against its `AppState`
//! registry and emitted `arbor://open-repo-tab { repo_id, path, display_name,
//! remote_url? }`. Here the `CorvusNsHost::tabs_open_repo` impl does the same
//! registry lookup + `self.state.emit(...)` in-process — no reverse-channel
//! round-trip, no shell handler. The frontend's AppShell listens on
//! `arbor://open-repo-tab` and runs the ensure-registered → activate/open flow.
//!
//! The error strings the shell produced are carried verbatim by the host
//! `Result<_, String>` and surfaced to Lua as the `(false, err)` second return:
//! `registry lock: …` (lock poisoned) / `repo '{repo_id}' not in registry`
//! (unknown id). The shell's `"app handle unavailable"` case has no analogue in
//! corvus-be (it always has a live state), so it never fires.

use mlua::{Lua, Table};

use arbor_plugin_core::prelude::{
    boolerr2, ApiCtx, LuaNamespaceInstaller, LuaTuple, PluginCoreError, PluginCoreResult,
};

use crate::nshost::NsHostHandle;

/// `arbor.tabs.*` installer. Holds the host handle the closures call through.
pub struct TabsInstaller {
    host: NsHostHandle,
}

impl TabsInstaller {
    pub fn new(host: NsHostHandle) -> Self {
        Self { host }
    }
}

impl LuaNamespaceInstaller for TabsInstaller {
    fn install(&self, ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> PluginCoreResult<()> {
        let tabs_table = lua
            .create_table()
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

        install_open_repo(self.host.clone(), ctx, lua, &tabs_table)?;

        arbor
            .set("tabs", tabs_table)
            .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
        Ok(())
    }
}

fn install_open_repo(
    host: NsHostHandle,
    _ctx: &ApiCtx,
    lua: &Lua,
    tabs_table: &Table,
) -> PluginCoreResult<()> {
    let fn_ = lua
        .create_function(move |lua_ctx, repo_id: String| -> LuaTuple {
            match host.tabs_open_repo(&repo_id) {
                Ok(()) => boolerr2(lua_ctx, true, None),
                Err(e) => boolerr2(lua_ctx, false, Some(e)),
            }
        })
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    tabs_table
        .set("open_repo", fn_)
        .map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
