//! `arbor.scheduler` — Spring-style background schedules.
//!
//! Manifest opt-in: `[scheduler] enabled = true` in plugin.toml. Without
//! that, every `register` call errors out — surfacing forgotten manifest
//! updates instead of silently dropping schedules on the floor.
//!
//!   arbor.scheduler.register({
//!     action            = "my_plugin:tick",   -- required, the action fired
//!     fixed_rate        = "5m",                -- "30s" / "5m" / "2h" / "1d"
//!     -- OR fixed_delay = "30s"                -- wait N AFTER handler returns
//!     -- OR cron        = "0 */5 * * * *"     -- 6-field Spring cron
//!     initial_delay     = "10s",               -- optional, default 0
//!     on_load           = false,               -- optional, fire once at load
//!     only_when_focused = true,                -- optional, skip when minimised
//!   })
//!
//!   arbor.scheduler.list() → { { action, trigger = { kind, … }, … }, … }

use std::str::FromStr;

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::convert::lua_value_to_duration_secs;
use crate::plugin::runtime::{PluginSchedule, ScheduleTrigger};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let sched_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;

    install_register(ctx, lua, &sched_table)?;
    install_list(ctx, lua, &sched_table)?;

    arbor.set("scheduler", sched_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_register(ctx: &ApiCtx, lua: &Lua, sched_table: &Table) -> Result<()> {
    let registry = ctx.schedules.clone();
    let pname    = ctx.plugin_name.clone();
    let scheduler_enabled = ctx.scheduler_enabled;
    let fn_ = lua.create_function(move |_, config: mlua::Table| {
        if !scheduler_enabled {
            return Err(mlua::Error::RuntimeError(format!(
                "arbor.scheduler.register: plugin '{pname}' must declare \
                 [scheduler] enabled = true in plugin.toml"
            )));
        }
        let action = parse_action(&config)?;
        let trigger = parse_trigger(&config)?;
        let initial_delay_sec = match config.get::<mlua::Value>("initial_delay").ok() {
            Some(mlua::Value::Nil) | None => 0,
            Some(v) => lua_value_to_duration_secs(&v).map_err(mlua::Error::RuntimeError)?,
        };
        let on_load           = config.get::<Option<bool>>("on_load").unwrap_or(None).unwrap_or(false);
        let only_when_focused = config.get::<Option<bool>>("only_when_focused").unwrap_or(None).unwrap_or(false);

        let schedule = PluginSchedule {
            action: action.clone(),
            trigger,
            initial_delay_sec,
            on_load,
            only_when_focused,
        };

        if let Ok(mut list) = registry.lock() {
            // Replace any existing entry with the same action so a
            // plugin can re-register from a hook to update parameters.
            list.retain(|s| s.action != action);
            list.push(schedule);
        }
        Ok(())
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sched_table.set("register", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_list(ctx: &ApiCtx, lua: &Lua, sched_table: &Table) -> Result<()> {
    let registry = ctx.schedules.clone();
    let fn_ = lua.create_function(move |lua_ctx, _: ()| {
        let list = registry.lock().map(|g| g.clone()).unwrap_or_default();
        let json = serde_json::to_value(&list).unwrap_or(serde_json::Value::Array(Vec::new()));
        Ok(lua_ctx.to_value(&json).unwrap_or(mlua::Value::Nil))
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    sched_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─── Parsing helpers ─────────────────────────────────────────────────────

fn parse_action(config: &mlua::Table) -> mlua::Result<String> {
    let action: String = config.get("action").map_err(|_| {
        mlua::Error::RuntimeError(
            "arbor.scheduler.register: 'action' (string) is required".into()
        )
    })?;
    if action.trim().is_empty() {
        return Err(mlua::Error::RuntimeError(
            "arbor.scheduler.register: 'action' must not be empty".into()
        ));
    }
    Ok(action)
}

fn parse_trigger(config: &mlua::Table) -> mlua::Result<ScheduleTrigger> {
    let fixed_rate:   Option<mlua::Value> = config.get("fixed_rate").ok();
    let fixed_delay:  Option<mlua::Value> = config.get("fixed_delay").ok();
    let cron_expr:    Option<String>     = config.get("cron").ok();

    let trigger_count =
        fixed_rate.as_ref().map_or(0, |v| if matches!(v, mlua::Value::Nil) { 0 } else { 1 })
      + fixed_delay.as_ref().map_or(0, |v| if matches!(v, mlua::Value::Nil) { 0 } else { 1 })
      + cron_expr.as_ref().map_or(0, |s| if s.is_empty() { 0 } else { 1 });

    if trigger_count != 1 {
        return Err(mlua::Error::RuntimeError(
            "arbor.scheduler.register: exactly one of 'fixed_rate', \
             'fixed_delay', or 'cron' must be provided".into()
        ));
    }

    if let Some(v) = fixed_rate.filter(|v| !matches!(v, mlua::Value::Nil)) {
        let secs = lua_value_to_duration_secs(&v).map_err(mlua::Error::RuntimeError)?;
        if secs == 0 {
            return Err(mlua::Error::RuntimeError(
                "arbor.scheduler.register: 'fixed_rate' must be > 0".into()
            ));
        }
        Ok(ScheduleTrigger::FixedRate { interval_sec: secs })
    } else if let Some(v) = fixed_delay.filter(|v| !matches!(v, mlua::Value::Nil)) {
        let secs = lua_value_to_duration_secs(&v).map_err(mlua::Error::RuntimeError)?;
        if secs == 0 {
            return Err(mlua::Error::RuntimeError(
                "arbor.scheduler.register: 'fixed_delay' must be > 0".into()
            ));
        }
        Ok(ScheduleTrigger::FixedDelay { delay_sec: secs })
    } else {
        let expr = cron_expr.unwrap();
        // Validate eagerly so the plugin author sees the error at load time,
        // not silently inside the scheduler thread.
        cron::Schedule::from_str(&expr).map_err(|e| {
            mlua::Error::RuntimeError(format!(
                "arbor.scheduler.register: invalid cron expression '{expr}': {e}"
            ))
        })?;
        Ok(ScheduleTrigger::Cron { expr })
    }
}
