//! `arbor.properties_studio.open(opts)` — open a `.properties` document in
//! the Studio modal.
//!
//! Mirrors `arbor.yaml_studio.open` / `arbor.toml_studio.open`. The
//! plugin doesn't manipulate the document directly from Lua: the host
//! parses through `properties_studio` (lossless line view), keeps the
//! doc in `AppState.studio_registry`, and the Svelte modal pulls
//! children / values lazily over IPC.
//!
//! Usage:
//! ```lua
//!   arbor.properties_studio.open{ text = "server.port=8080\n", title = "scratch" }
//!   arbor.properties_studio.open{ path = "/abs/path/application.properties" }
//! ```
//!
//! The modal opens via the `arbor://properties-studio-open` Tauri
//! event, routed in AppShell into `propertiesStudioStore.openDoc(...)`.

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let open = lua.create_function(move |_, opts: mlua::Value| {
        let cfg = match opts {
            mlua::Value::Table(t) => t,
            _ => return Err(mlua::Error::RuntimeError(
                "arbor.properties_studio.open: expected a table { text= or path=, title? }".into()
            )),
        };
        let text:  Option<String> = cfg.get::<Option<String>>("text").ok().flatten();
        let path:  Option<String> = cfg.get::<Option<String>>("path").ok().flatten();
        let title: Option<String> = cfg.get::<Option<String>>("title").ok().flatten();
        if text.is_none() && path.is_none() {
            return Err(mlua::Error::RuntimeError(
                "arbor.properties_studio.open: provide `text` or `path`".into()
            ));
        }
        if let Some(ref h) = handle {
            let payload = serde_json::json!({
                "plugin": pname,
                "text":   text,
                "path":   path,
                "title":  title,
            });
            let _ = h.emit("arbor://properties-studio-open", payload);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("open", open).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("properties_studio", table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
