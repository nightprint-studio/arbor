//! `arbor.ui.operation.*` — push entries into the global operations
//! overlay (the floating cards used by single-repo Pull, workspace
//! Fetch-all / Pull-all, linked-worktree sync). Plugins use it for
//! long-running async work that wants a visible progress card with a
//! step strip, instead of just a one-shot toast.
//!
//! Surface (Lua):
//!   arbor.ui.operation.start{
//!     id       = "release-notes:run-1",
//!     title    = "Generate release notes",
//!     subtitle = "v1.0.0 → HEAD",
//!     steps    = {
//!       { key = "commits", label = "Reading commits"  },
//!       { key = "tickets", label = "Enriching tickets" },
//!       { key = "render",  label = "Building markdown" },
//!       { key = "write",   label = "Writing file"     },
//!     },
//!     current = "commits",
//!   }
//!
//!   arbor.ui.operation.set_current(id, step_key, detail?)
//!   arbor.ui.operation.update_step(id, step_key, { status?, detail? })
//!   arbor.ui.operation.finish(id, { summary?, error? })
//!
//! Frontend wiring lives in `src/lib/utils/operations-bridge.ts` —
//! the `arbor://plugin-operation-*` events are routed straight into
//! `operationsStore` so plugin progress cards look identical to the
//! built-in pull / fetch ones (no separate widget).
//!
//! No permission gate: the API is purely visual (no git / fs / network
//! side-effects on its own). Misuse just clutters the overlay.

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let op_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_start(ctx, lua, &op_table)?;
    install_set_current(ctx, lua, &op_table)?;
    install_update_step(ctx, lua, &op_table)?;
    install_finish(ctx, lua, &op_table)?;

    ui.set("operation", op_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

/// Build a stable plugin-scoped id so two plugins can't collide on the
/// same operation key. Plugin authors pass any string; we prepend the
/// plugin name to keep the registry namespace flat.
fn scoped_id(plugin: &str, raw: &str) -> String {
    if raw.is_empty() { format!("{plugin}:auto") } else { format!("{plugin}:{raw}") }
}

fn install_start(ctx: &ApiCtx, lua: &Lua, op_table: &Table) -> Result<()> {
    let pname  = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: Table| {
        let raw_id: String = cfg.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.operation.start: 'id' is required".into())
        })?;
        let title: String = cfg.get::<String>("title").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.operation.start: 'title' is required".into())
        })?;
        let subtitle: Option<String> = cfg.get::<Option<String>>("subtitle").ok().flatten();
        let current:  Option<String> = cfg.get::<Option<String>>("current").ok().flatten();

        // Steps must be an array of { key, label, detail?, status? } tables.
        let steps_tbl: Table = cfg.get("steps").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.operation.start: 'steps' is required (array)".into())
        })?;
        let mut steps: Vec<serde_json::Value> = Vec::new();
        for pair in steps_tbl.sequence_values::<Table>() {
            let st: Table = match pair { Ok(t) => t, Err(_) => continue };
            let key:    String         = st.get::<String>("key").unwrap_or_default();
            let label:  String         = st.get::<String>("label").unwrap_or_else(|_| key.clone());
            let detail: Option<String> = st.get::<Option<String>>("detail").ok().flatten();
            let status: Option<String> = st.get::<Option<String>>("status").ok().flatten();
            if key.is_empty() { continue; } // silently skip malformed rows
            let mut entry = serde_json::json!({ "key": key, "label": label });
            if let Some(d) = detail { entry["detail"] = serde_json::Value::String(d); }
            if let Some(s) = status { entry["status"] = serde_json::Value::String(s); }
            steps.push(entry);
        }
        if steps.is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.ui.operation.start: 'steps' must contain at least one { key, label } row".into()
            ));
        }

        let _ = lua_ctx; // unused — kept for symmetry with other installers
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://plugin-operation-start", serde_json::json!({
                "id":       scoped_id(&pname, &raw_id),
                "plugin":   &pname,
                "title":    title,
                "subtitle": subtitle,
                "steps":    steps,
                "current":  current,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    op_table.set("start", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_set_current(ctx: &ApiCtx, lua: &Lua, op_table: &Table) -> Result<()> {
    let pname  = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, args: mlua::MultiValue| {
        // Accept (id, step_key) or (id, step_key, detail).
        let mut it = args.into_iter();
        let id: String = match it.next() {
            Some(mlua::Value::String(s)) => s.to_str().map(|x| x.to_string()).unwrap_or_default(),
            _ => return Err(mlua::Error::RuntimeError(
                "arbor.ui.operation.set_current(id, step_key, detail?): 'id' must be a string".into())),
        };
        let step: String = match it.next() {
            Some(mlua::Value::String(s)) => s.to_str().map(|x| x.to_string()).unwrap_or_default(),
            _ => return Err(mlua::Error::RuntimeError(
                "arbor.ui.operation.set_current(id, step_key, detail?): 'step_key' must be a string".into())),
        };
        let detail: Option<String> = match it.next() {
            Some(mlua::Value::String(s)) => s.to_str().map(|x| x.to_string()).ok(),
            _ => None,
        };
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://plugin-operation-update", serde_json::json!({
                "id":      scoped_id(&pname, &id),
                "plugin":  &pname,
                "kind":    "set_current",
                "step":    step,
                "detail":  detail,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    op_table.set("set_current", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_update_step(ctx: &ApiCtx, lua: &Lua, op_table: &Table) -> Result<()> {
    let pname  = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (id, step, patch): (String, String, Table)| {
        let status: Option<String> = patch.get::<Option<String>>("status").ok().flatten();
        let detail: Option<String> = patch.get::<Option<String>>("detail").ok().flatten();
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://plugin-operation-update", serde_json::json!({
                "id":     scoped_id(&pname, &id),
                "plugin": &pname,
                "kind":   "update_step",
                "step":   step,
                "status": status,
                "detail": detail,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    op_table.set("update_step", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_finish(ctx: &ApiCtx, lua: &Lua, op_table: &Table) -> Result<()> {
    let pname  = ctx.plugin_name.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (id, opts): (String, Option<Table>)| {
        let (summary, error) = match opts {
            Some(t) => (
                t.get::<Option<String>>("summary").ok().flatten(),
                t.get::<Option<String>>("error").ok().flatten(),
            ),
            None => (None, None),
        };
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://plugin-operation-finish", serde_json::json!({
                "id":      scoped_id(&pname, &id),
                "plugin":  &pname,
                "summary": summary,
                "error":   error,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    op_table.set("finish", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
