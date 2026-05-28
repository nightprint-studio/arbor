//! `arbor.timer.after` / `arbor.timer.every` / `arbor.timer.cancel`.

use std::sync::atomic::Ordering;

use mlua::{Lua, Table};

use crate::error::{PluginCoreError, Result};
use crate::lua_api::ctx::ApiCtx;
use crate::lua_api::helpers::timer::{register_timer_cancel, register_timer_hook};

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let timer_table = lua.create_table().map_err(|e| PluginCoreError::Plugin(e.to_string()))?;

    install_after(ctx, lua, &timer_table)?;
    install_every(ctx, lua, &timer_table)?;
    install_cancel(ctx, lua, &timer_table)?;

    arbor.set("timer", timer_table).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_after(ctx: &ApiCtx, lua: &Lua, timer_table: &Table) -> Result<()> {
    let host     = ctx.host_weak.clone();
    let pname    = ctx.plugin_name.clone();
    let cancels  = ctx.timer_cancels.clone();
    let counter  = ctx.timer_counter.clone();

    let after_fn = lua.create_function(move |lua_ctx, (delay_ms, func): (u64, mlua::Function)| {
        let id = format!("__timer_{}__", counter.fetch_add(1, Ordering::Relaxed));
        register_timer_hook(lua_ctx, &id, func)?;
        let cancel = register_timer_cancel(&cancels, &id)?;
        if let Some(weak) = host.clone() {
            let pn      = pname.clone();
            let hook_id = id.clone();
            let tc      = cancels.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if cancel.load(Ordering::Relaxed) { return; }
                if let Some(arc) = weak.upgrade() {
                    if let Ok(host) = arc.lock() {
                        let _ = host.fire_hook_on(&pn, &hook_id, "{}");
                    }
                }
                // Clean up cancel token.
                if let Ok(mut tc) = tc.lock() { tc.remove(&hook_id); }
            });
        }
        Ok(lua_ctx.create_string(id.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    timer_table.set("after", after_fn).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_every(ctx: &ApiCtx, lua: &Lua, timer_table: &Table) -> Result<()> {
    let host    = ctx.host_weak.clone();
    let pname   = ctx.plugin_name.clone();
    let cancels = ctx.timer_cancels.clone();
    let counter = ctx.timer_counter.clone();

    let every_fn = lua.create_function(move |lua_ctx, (interval_ms, func): (u64, mlua::Function)| {
        let id = format!("__timer_{}__", counter.fetch_add(1, Ordering::Relaxed));
        register_timer_hook(lua_ctx, &id, func)?;
        let cancel = register_timer_cancel(&cancels, &id)?;
        if let Some(weak) = host.clone() {
            let pn      = pname.clone();
            let hook_id = id.clone();
            std::thread::spawn(move || {
                loop {
                    // Sleep in 50 ms increments to check cancel flag.
                    let mut slept = 0u64;
                    while slept < interval_ms {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if cancel.load(Ordering::Relaxed) { return; }
                        slept += 50;
                    }
                    if let Some(arc) = weak.upgrade() {
                        if let Ok(host) = arc.lock() {
                            let _ = host.fire_hook_on(&pn, &hook_id, "{}");
                        }
                    } else {
                        return;
                    }
                }
            });
        }
        Ok(lua_ctx.create_string(id.as_bytes())?)
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    timer_table.set("every", every_fn).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_cancel(ctx: &ApiCtx, lua: &Lua, timer_table: &Table) -> Result<()> {
    let cancels = ctx.timer_cancels.clone();
    let cancel_fn = lua.create_function(move |_, id: String| {
        if let Ok(map) = cancels.lock() {
            if let Some(token) = map.get(&id) {
                token.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    timer_table.set("cancel", cancel_fn).map_err(|e| PluginCoreError::Plugin(e.to_string()))?;
    Ok(())
}
