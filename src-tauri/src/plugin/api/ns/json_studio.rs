//! `arbor.json_studio.open(opts)` — open a JSON document in the Studio modal.
//!
//! The plugin doesn't manipulate the document directly from Lua. The host
//! parses with simd-json (huge speedup vs Lua), keeps the tree in
//! `AppState.json_studio`, and the Svelte modal pulls children/values
//! lazily over IPC. The Lua API is intentionally a single open call —
//! anything richer would push parser-specific concerns into Lua, which is
//! what the planned WASM migration will solve properly.
//!
//! Usage:
//! ```lua
//!   arbor.json_studio.open{ text = "{\"a\":1}",                     title = "scratch" }
//!   arbor.json_studio.open{ path = "/abs/path/to/data.json" }       -- title defaults to filename
//! ```
//!
//! The modal opens via the `arbor://json-studio-open` Tauri event, which
//! the AppShell listener routes into `jsonStudioStore.open(...)`.

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
                "arbor.json_studio.open: expected a table { text= or path=, title? }".into()
            )),
        };
        let text:  Option<String> = cfg.get::<Option<String>>("text").ok().flatten();
        let path:  Option<String> = cfg.get::<Option<String>>("path").ok().flatten();
        let title: Option<String> = cfg.get::<Option<String>>("title").ok().flatten();
        if text.is_none() && path.is_none() {
            return Err(mlua::Error::RuntimeError(
                "arbor.json_studio.open: provide `text` or `path`".into()
            ));
        }
        if let Some(ref h) = handle {
            let payload = serde_json::json!({
                "plugin": pname,
                "text":   text,
                "path":   path,
                "title":  title,
            });
            let _ = h.emit("arbor://json-studio-open", payload);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    table.set("open", open).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("json_studio", table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
