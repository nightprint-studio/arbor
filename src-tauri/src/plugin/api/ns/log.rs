//! `arbor.log.{debug,info,warn,error}` + `arbor.log.LEVELS`.

use std::sync::atomic::Ordering;

use mlua::{Lua, Table};

use crate::error::{AppError, Result};

use crate::plugin::api::ctx::ApiCtx;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let log_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    for level in &["debug", "info", "warn", "error"] {
        let lvl       = level.to_string();
        let pname     = ctx.plugin_name.clone();
        let handle    = ctx.app_handle.clone();
        let enabled_c = ctx.enabled.clone();
        let log_fn = lua
            .create_function(move |_, msg: String| {
                // Disabled plugins must not leak into tracing or the
                // Plugin Logs panel — even if a stray timer or scheduler
                // tick races with `disable_plugin`. Safety net for the
                // skip-load path: an in-flight closure observes the flag
                // flip immediately.
                if !enabled_c.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match lvl.as_str() {
                    "debug" => tracing::debug!(target: "plugin", "[{pname}] {msg}"),
                    "warn"  => tracing::warn! (target: "plugin", "[{pname}] {msg}"),
                    "error" => tracing::error!(target: "plugin", "[{pname}] {msg}"),
                    _       => tracing::info! (target: "plugin", "[{pname}] {msg}"),
                }
                if let Some(h) = handle.as_ref() {
                    crate::plugin_logs::record(h, &lvl, &pname, msg);
                }
                Ok(())
            })
            .map_err(|e| AppError::Plugin(e.to_string()))?;
        log_table.set(*level, log_fn).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    let levels = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    levels.set("DEBUG", "debug").map_err(|e| AppError::Plugin(e.to_string()))?;
    levels.set("INFO",  "info" ).map_err(|e| AppError::Plugin(e.to_string()))?;
    levels.set("WARN",  "warn" ).map_err(|e| AppError::Plugin(e.to_string()))?;
    levels.set("ERROR", "error").map_err(|e| AppError::Plugin(e.to_string()))?;
    log_table.set("LEVELS", levels).map_err(|e| AppError::Plugin(e.to_string()))?;

    arbor.set("log", log_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
