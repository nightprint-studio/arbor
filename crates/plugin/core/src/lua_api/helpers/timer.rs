//! Timer-registration helpers shared by `arbor.timer.*` and `arbor.job.spawn`.

use std::sync::Arc;

use mlua::{Lua, Table};

use crate::runtime::{TimerCancel, TimerCancels};

/// Park a one-shot Lua function under `__arbor_hooks__[id]` so the timer
/// thread can fire it via `hook_router::fire_on(&host, plugin, id, "{}")`.
pub fn register_timer_hook(lua: &Lua, id: &str, func: mlua::Function) -> mlua::Result<()> {
    let registry: Table = lua.globals().get("__arbor_hooks__")?;
    let list = lua.create_table()?;
    list.push(func)?;
    registry.set(id, list)?;
    Ok(())
}

/// Allocate a cancel token for a timer/scheduler entry and stash it in the
/// global registry so `arbor.timer.cancel(id)` can trip it asynchronously.
pub fn register_timer_cancel(
    cancels: &TimerCancels,
    id: &str,
) -> mlua::Result<Arc<TimerCancel>> {
    let cancel = TimerCancel::new();
    cancels.lock()
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
        .insert(id.to_string(), cancel.clone());
    Ok(cancel)
}
