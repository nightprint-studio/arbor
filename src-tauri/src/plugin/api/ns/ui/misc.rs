//! Miscellaneous one-off `arbor.ui.*` helpers:
//!   - arbor.ui.open_path
//!   - arbor.ui.copy_to_clipboard
//!   - arbor.ui.show_pipeline_run
//!   - arbor.ui.open_job_output
//!   - arbor.ui.open_panel

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    install_open_path(ctx, lua, ui)?;
    install_copy_to_clipboard(ctx, lua, ui)?;
    install_show_pipeline_run(ctx, lua, ui)?;
    install_open_job_output(ctx, lua, ui)?;
    install_open_panel(ctx, lua, ui)?;
    Ok(())
}

fn install_open_path(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // open_path(path) — hand a file/folder to the OS' default handler.
    // On Windows this reveals a folder in Explorer or opens a file with
    // its registered application; equivalent xdg-open / open behavior on
    // Linux / macOS. Useful for plugins that produce artefact folders
    // and want to expose a one-click "Open in file manager" affordance.
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, path: String| {
        let Some(ref h) = handle else {
            return Err(mlua::Error::RuntimeError(
                "arbor.ui.open_path requires a running app handle".to_string()));
        };
        if path.is_empty() {
            return Err(mlua::Error::RuntimeError(
                "arbor.ui.open_path: path cannot be empty".to_string()));
        }
        use tauri_plugin_opener::OpenerExt;
        h.opener().open_path(&path, None::<&str>)
            .map_err(|e| mlua::Error::RuntimeError(
                format!("arbor.ui.open_path failed: {e}")))?;
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("open_path", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_copy_to_clipboard(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // copy_to_clipboard{text, toast?} — emit an event picked up by AppShell
    // which writes to the browser clipboard API and surfaces a success
    // toast. Running it server-side would need a clipboard crate; going
    // via the frontend reuses the browser's permission model and keeps
    // this API sandbox-friendly.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, cfg: mlua::Table| {
        let text: String = cfg.get("text").map_err(|_|
            mlua::Error::RuntimeError("arbor.ui.copy_to_clipboard: 'text' is required".into()))?;
        let toast_msg: Option<String> = cfg.get::<Option<String>>("toast").unwrap_or(None);
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:ui-clipboard-write", serde_json::json!({
                "plugin": pname,
                "text":   text,
                "toast":  toast_msg.unwrap_or_else(|| "Copied to clipboard".to_string()),
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("copy_to_clipboard", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_open_job_output(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // open_job_output(job_id) — ask the frontend to load the job's output
    // buffer and surface the JobOutputPanel in the bottom slot. No-op on
    // empty id; AppShell handles missing jobs by showing a "not found"
    // message in the panel (the call itself never fails).
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, job_id: String| {
        if job_id.is_empty() { return Ok(()); }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:ui-open-job-output", serde_json::json!({
                "plugin": pname,
                "job_id": job_id,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("open_job_output", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_open_panel(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // open_panel(panel_id) — programmatically reveal the plugin's own
    // sidebar panel (whichever side/position it was registered with).
    // Use case: "show details on selection" UX where the detail panel
    // lives in a bottom section and should auto-open when the user clicks
    // an entity in the top sidebar. AppShell parses the registered
    // sidebar contribution to decide left / right / bottom dispatch — the
    // plugin doesn't have to know its own side here.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, panel_id: String| {
        if panel_id.is_empty() { return Ok(()); }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:ui-open-panel", serde_json::json!({
                "plugin":   pname,
                "panel_id": panel_id,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("open_panel", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_show_pipeline_run(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    // show_pipeline_run(run_id) — ask the frontend to open the
    // Pipelines bottom panel and focus the given run. No-op when the
    // run id is empty; AppShell decides whether to scroll / highlight.
    let handle = ctx.app_handle.clone();
    let pname  = ctx.plugin_name.clone();
    let fn_ = lua.create_function(move |_, run_id: String| {
        if run_id.is_empty() { return Ok(()); }
        if let Some(ref h) = handle {
            let _ = h.emit("plugin:ui-show-pipeline-run", serde_json::json!({
                "plugin": pname,
                "run_id": run_id,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    ui.set("show_pipeline_run", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
