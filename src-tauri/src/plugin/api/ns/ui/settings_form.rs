//! `arbor.ui.settings` — sugar over `arbor.ui.container` for the
//! conventional "plugin settings" surface.
//!
//!   arbor.ui.settings.panel({
//!     id           = "main",
//!     title        = "Compile · Build & Toolchains",
//!     width        = "960px",
//!     submit_label = "Close",
//!     on_load      = "compile:settings_refresh",
//!     on_save      = nil,        -- per-section on_save handles persistence
//!   })
//!   arbor.ui.settings.open(plugin_name, panel_id)
//!   arbor.ui.settings.close()
//!
//! The wrapper registers a container whose category/section sub-points are
//! forced to the historical naming `<plugin>:settings:category` and
//! `<plugin>:settings:section` — so any plugin contributing under the old
//! single-colon convention keeps working without source changes.
//! `arbor.ui.settings.open` simply emits `arbor://container-open` with the
//! canonical `<plugin>::<panel_id>` key; `arbor.ui.settings.close` emits
//! `arbor://container-close` with an empty id, which the store interprets
//! as "close whatever is currently open".

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::contribution::ContainerDef;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let settings_ui = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_panel(ctx, lua, &settings_ui)?;
    install_open(ctx, lua, &settings_ui)?;
    install_close(ctx, lua, &settings_ui)?;

    ui.set("settings", settings_ui).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_panel(ctx: &ApiCtx, lua: &Lua, settings_ui: &Table) -> Result<()> {
    // panel(config) — registers a container with the settings naming
    // contract baked in.
    let pname  = ctx.plugin_name.clone();
    let reg    = ctx.contributions.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError("arbor.ui.settings.panel: 'id' is required".to_string())
        })?;
        let title = config.get::<Option<String>>("title").unwrap_or(None)
            .unwrap_or_else(|| format!("{} settings", pname));
        let width        = config.get::<Option<String>>("width").unwrap_or(None);
        let height       = config.get::<Option<String>>("height").unwrap_or(None);
        let submit_label = config.get::<Option<String>>("submit_label").unwrap_or(None);
        let cancel_label = config.get::<Option<String>>("cancel_label").unwrap_or(None);
        let on_save      = config.get::<Option<String>>("on_save").unwrap_or(None);
        let on_load      = config.get::<Option<String>>("on_load").unwrap_or(None);

        let def = ContainerDef {
            key:            String::new(),  // filled in by register_container
            plugin_name:    pname.clone(),
            id:             id.clone(),
            kind:           "modal".to_string(),
            layout:         "tree_nav".to_string(),
            title, width, height, submit_label, cancel_label,
            on_save, on_load,
            // Force legacy single-colon naming so existing plugins'
            // contributions to "<plugin>:settings:category|section"
            // continue to populate the modal.
            category_point: Some(format!("{}:settings:category", pname)),
            section_point:  Some(format!("{}:settings:section",  pname)),
        };
        reg.register_container(def);
        reg.notify_changed(&handle, "arbor:container");
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_ui.set("panel", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_open(ctx: &ApiCtx, lua: &Lua, settings_ui: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, (target_plugin, panel_id): (String, String)| {
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://container-open", serde_json::json!({
                "container_id": format!("{}::{}", target_plugin, panel_id),
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_ui.set("open", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_close(ctx: &ApiCtx, lua: &Lua, settings_ui: &Table) -> Result<()> {
    // close() — emits arbor://container-close with an empty id, meaning
    // "close whatever is currently open" (the store handles the wildcard).
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, ()| {
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://container-close", serde_json::json!({
                "container_id": "",
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    settings_ui.set("close", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
