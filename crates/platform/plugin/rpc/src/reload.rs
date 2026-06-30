//! Whole-runtime mutations — reload + the master kill-switch. Generic port of
//! `corvus-be`'s former `plugin_reload.rs`.
//!
//! * `reload_plugins` — full `host.reload()` + restart schedulers, then re-fire
//!   `on_repo_open` for every open tab and broadcast `arbor://plugins-reloaded`.
//! * `set_plugins_enabled` — master toggle. Persists the choice over the reverse
//!   channel (`__set_plugins_enabled`, idempotent compare-and-save) **before**
//!   mutating the runtime, then reloads (on) or tears down via `unload_all` (off).
//!
//! Order matters: rebuild the host + restart schedulers **under the lock**, drop
//! the lock, *then* fire the per-tab lifecycle hooks and emit — firing hooks
//! while holding the host lock would deadlock (the broker re-enters the host's
//! own VMs). `plugin_states.json` is untouched (per-plugin state is owned by
//! enable/disable); `reload()`/`unload_all()` only read/clear memory.

use serde_json::{json, Value};

use crate::context::{with_host_mut, PluginRpcContext};

/// Rebuild the plugin runtime from disk and re-announce it — shared by
/// `reload_plugins` and the "enable" branch of `set_plugins_enabled`.
pub fn reload_runtime<C: PluginRpcContext>(ctx: &C) -> Result<(), String> {
    // Rebuild + restart schedulers under the lock, then drop the guard before
    // firing any hooks (the broker re-enters the host).
    with_host_mut(ctx, |host| {
        host.reload().map_err(|e| e.to_string())?;
        host.start_all_schedulers();
        Ok(())
    })?;

    // Re-fire `on_repo_open` for every open tab so plugins that derive
    // `current_repo` from the last lifecycle event re-acquire it on the freshly
    // rebuilt VMs (the broadcast path also refreshes `__arbor_current_repo__`).
    for r in ctx.open_repos() {
        ctx.fire_hook(
            "on_repo_open",
            json!({ "tab_id": r.tab_id, "path": r.path, "name": r.name }),
        );
    }

    ctx.emit("arbor://plugins-reloaded", Value::Null);
    Ok(())
}

/// Reload all plugins from disk: rebuild the host, restart schedulers, re-fire
/// `on_repo_open` for open tabs, and broadcast `arbor://plugins-reloaded`.
pub fn reload_plugins<C: PluginRpcContext>(ctx: &C) -> Result<(), String> {
    reload_runtime(ctx)
}

/// Master plugin-system kill-switch. Persists the choice (over the reverse
/// channel, idempotent), then either reloads the runtime (`enabled`) or tears it
/// down (`!enabled`). The persist is a non-fatal best-effort: on failure we warn
/// and apply the runtime mutation anyway, mirroring the shell.
pub fn set_plugins_enabled<C: PluginRpcContext>(ctx: &C, enabled: bool) -> Result<(), String> {
    // Persist before touching the runtime so a crash in between can't desync the
    // saved flag. The shell owns the typed config; the reverse method does the
    // compare-save and reports whether anything changed.
    let changed: bool = match ctx.host_call("__set_plugins_enabled", json!({ "enabled": enabled })) {
        Ok(v) => serde_json::from_value(v).unwrap_or(true),
        Err(e) => {
            eprintln!("plugin-rpc: failed to persist plugins_enabled: {e}");
            true
        }
    };
    if !changed {
        // Stored value already matched → nothing to apply (shell's early return).
        return Ok(());
    }

    if enabled {
        reload_runtime(ctx)?;
    } else {
        // Tear the runtime down. `unload_all` fires `on_plugin_unload` and drops
        // contributions/tree/icons internally.
        with_host_mut(ctx, |host| {
            host.unload_all();
            Ok(())
        })?;
        ctx.emit("arbor://plugins-reloaded", Value::Null);
    }
    Ok(())
}
