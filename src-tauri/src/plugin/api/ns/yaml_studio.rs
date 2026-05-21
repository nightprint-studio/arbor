//! `arbor.yaml_studio.open(opts)` — open a YAML document in the Studio modal.
//!
//! Mirrors `arbor.ron_studio.open` / `arbor.json_studio.open` /
//! `arbor.toml_studio.open` exactly. The plugin doesn't manipulate the
//! document directly from Lua: the host parses through `serde_yaml_ng`
//! (read-only navigation in Phase 5.a — lossless edit lands in 5.b),
//! keeps the document in `AppState.studio_registry`, and the Svelte
//! modal pulls children / values lazily over IPC. Save/edit/diff all
//! live on the host + frontend; the plugin is purely a launcher.
//!
//! Usage:
//! ```lua
//!   arbor.yaml_studio.open{ text = "key: value\n", title = "scratch" }
//!   arbor.yaml_studio.open{ path = "/abs/path/config.yaml" }
//! ```
//!
//! The modal opens via the `arbor://yaml-studio-open` Tauri event,
//! which the AppShell listener routes into `yamlStudioStore.openDoc(...)`.

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
                "arbor.yaml_studio.open: expected a table { text= or path=, title? }".into()
            )),
        };
        let text:  Option<String> = cfg.get::<Option<String>>("text").ok().flatten();
        let path:  Option<String> = cfg.get::<Option<String>>("path").ok().flatten();
        let title: Option<String> = cfg.get::<Option<String>>("title").ok().flatten();
        if text.is_none() && path.is_none() {
            return Err(mlua::Error::RuntimeError(
                "arbor.yaml_studio.open: provide `text` or `path`".into()
            ));
        }
        if let Some(ref h) = handle {
            let payload = serde_json::json!({
                "plugin": pname,
                "text":   text,
                "path":   path,
                "title":  title,
            });
            let _ = h.emit("arbor://yaml-studio-open", payload);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("open", open).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("yaml_studio", table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
