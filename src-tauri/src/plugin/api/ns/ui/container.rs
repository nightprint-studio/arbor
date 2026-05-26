//! `arbor.ui.container.{register, open, close}`.
//!
//! A container is an aggregated UI surface (modal / sidebar in the future)
//! whose body is built from contributions to:
//!   "<plugin>::<container_id>:category"   { id, label, icon?, … }
//!   "<plugin>::<container_id>:section"    { category?, label?, nodes?, on_save?, … }
//!
//! The frontend listens for `arbor://container-open` and mounts
//! ContributableModal with the supplied key. `key` is the canonical id
//! `"<plugin>::<id>"` — register() builds it from the calling plugin's
//! name + the supplied id, so two plugins can use the same local id
//! without colliding.

use mlua::{Lua, Table};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::contribution::ContainerDef;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, ui: &Table) -> Result<()> {
    let container_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_register(ctx, lua, &container_table)?;
    install_open(ctx, lua, &container_table)?;
    install_close(ctx, lua, &container_table)?;

    ui.set("container", container_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register(ctx: &ApiCtx, lua: &Lua, container_table: &Table) -> Result<()> {
    let pname  = ctx.plugin_name.clone();
    let reg    = ctx.contributions.clone();
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        let id = config.get::<String>("id").map_err(|_| {
            mlua::Error::RuntimeError(
                "arbor.ui.container.register: 'id' is required".to_string()
            )
        })?;
        let title = config.get::<String>("title").map_err(|_| {
            mlua::Error::RuntimeError(
                "arbor.ui.container.register: 'title' is required".to_string()
            )
        })?;
        let kind           = config.get::<Option<String>>("kind").unwrap_or(None)
                               .unwrap_or_else(|| "modal".to_string());
        let layout         = config.get::<Option<String>>("layout").unwrap_or(None)
                               .unwrap_or_else(|| "tree_nav".to_string());
        let width          = config.get::<Option<String>>("width").unwrap_or(None);
        let height         = config.get::<Option<String>>("height").unwrap_or(None);
        let submit_label   = config.get::<Option<String>>("submit_label").unwrap_or(None);
        let cancel_label   = config.get::<Option<String>>("cancel_label").unwrap_or(None);
        let on_save        = config.get::<Option<String>>("on_save").unwrap_or(None);
        let on_load        = config.get::<Option<String>>("on_load").unwrap_or(None);
        let category_point = config.get::<Option<String>>("category_point").unwrap_or(None);
        let section_point  = config.get::<Option<String>>("section_point").unwrap_or(None);
        let def = ContainerDef {
            key:         String::new(),  // filled in by register_container
            plugin_name: pname.clone(),
            id:          id.clone(),
            kind, layout, title, width, height, submit_label, cancel_label,
            on_save, on_load, category_point, section_point,
        };
        if reg.register_container(def) {
            reg.notify_containers_changed(&handle);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    container_table.set("register", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_open(ctx: &ApiCtx, lua: &Lua, container_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, key: String| {
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://container-open", serde_json::json!({
                "container_id": key,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    container_table.set("open", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_close(ctx: &ApiCtx, lua: &Lua, container_table: &Table) -> Result<()> {
    let handle = ctx.app_handle.clone();
    let fn_ = lua.create_function(move |_, key: String| {
        if let Some(ref h) = handle {
            let _ = h.emit("arbor://container-close", serde_json::json!({
                "container_id": key,
            }));
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    container_table.set("close", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
