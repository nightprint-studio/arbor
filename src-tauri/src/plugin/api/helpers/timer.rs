//! Timer-registration helpers shared by `arbor.timer.*` and `arbor.job.spawn`.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use mlua::{Lua, Table};

use crate::plugin::runtime::TimerCancels;

/// Park a one-shot Lua function under `__arbor_hooks__[id]` so the timer
/// thread can fire it via `host.fire_hook_on(plugin, id, "{}")`.
pub(crate) fn register_timer_hook(lua: &Lua, id: &str, func: mlua::Function) -> mlua::Result<()> {
    let registry: Table = lua.globals().get("__arbor_hooks__")?;
    let list = lua.create_table()?;
    list.push(func)?;
    registry.set(id, list)?;
    Ok(())
}

/// Allocate a cancel flag for a timer/scheduler entry and stash it in the
/// global registry so `arbor.timer.cancel(id)` can flip it asynchronously.
pub(crate) fn register_timer_cancel(
    cancels: &TimerCancels,
    id: &str,
) -> mlua::Result<Arc<AtomicBool>> {
    let cancel = Arc::new(AtomicBool::new(false));
    cancels.lock()
        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?
        .insert(id.to_string(), cancel.clone());
    Ok(cancel)
}
