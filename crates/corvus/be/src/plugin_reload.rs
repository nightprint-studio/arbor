//! Plugin-Manager **reload / master-toggle** ops over the OOP boundary — the
//! runtime-rebuild twin of [`crate::plugin_lifecycle`] (which ports the
//! per-plugin enable/disable cascade).
//!
//! After the Phase-2 flip the shell loads NO Corvus product plugins; the live
//! host is owned by `main` here in `corvus-be`. `plugin_introspect` re-served the
//! **read/reflection** subset and `plugin_lifecycle` the **enable/disable**
//! subset; this module re-serves the two **whole-runtime** mutations the Plugin
//! Manager exposes:
//!
//! * `reload_plugins` — full `host.reload()` + restart schedulers, then re-fire
//!   `on_repo_open` for every open tab and broadcast `arbor://plugins-reloaded`.
//! * `set_plugins_enabled` — the master kill-switch. When turned **on** it runs
//!   the same reload; when turned **off** it tears the runtime down
//!   (`host.unload_all()`) and broadcasts `arbor://plugins-reloaded`. The choice
//!   is persisted **before** the runtime mutation so a crash in between can't
//!   leave the saved flag out of sync with what was applied.
//!
//! ## Accessor — single source of truth
//!
//! Same module-static `Arc<Mutex<PluginHost>>` every other corvus-be plugin
//! handler uses: the one `plugin_introspect::install(...)` is handed at boot,
//! borrowed via the `pub(crate)` accessor [`crate::plugin_introspect::host`]. No
//! second `install`/static. The reload/toggle ops also need the event sink + the
//! open-tab set + the hook broker, so — unlike the host-only enable/disable —
//! their handlers take `ctx: &CorvusState` and reach those through it.
//!
//! ## Faithfulness contract vs. the shell
//!
//! Mirrors `src-tauri/src/ipc/platform/plugin.rs`'s `reload_runtime` /
//! `set_plugins_enabled` / `reload_plugins`:
//!
//! * **`plugin_states.json`** is untouched by these ops (per-plugin enable-state
//!   is owned by `enable_plugin`/`disable_plugin`); `reload()`/`unload_all()` read
//!   it / clear memory but don't rewrite it — same as the shell.
//! * **`host.reload()` then `host.start_all_schedulers()`** — identical to the
//!   shell's reload body *and* to corvus-be's own boot `on_ready` reload in
//!   `main.rs`, so a manual reload and a boot reload converge on the same state.
//! * **`arbor://plugins-reloaded`** is emitted on every path that mutates the
//!   runtime — both the reload path and the disable path — exactly as the shell.
//!   `CorvusState::emit` takes a JSON payload, so the shell's `()` unit payload is
//!   sent as `Value::Null` (the serialized form of `()` over Tauri is also
//!   `null`, so the FE listener — which ignores the payload — sees no difference).
//! * **`on_repo_open` re-fire** for every open tab keeps plugins that derive
//!   `current_repo` from the last lifecycle event landing on a real repo after the
//!   VMs are rebuilt. The broadcast path refreshes each plugin's
//!   `__arbor_current_repo__` global as a side effect (see
//!   `crates/plugin/core/src/hook_router.rs`), so we fire the hook and let that
//!   path set the global rather than poking it directly.
//!
//! ### Two faithful divergences forced by corvus-be's narrower state
//!
//! 1. **No plugin-job cancellation.** The shell's `reload_runtime` /
//!    `set_plugins_enabled(false)` first call `state.jobs.cancel_by_plugin(None)`
//!    against the shell's local `JobRegistry`. corvus-be keeps **no** local job
//!    registry — jobs live in the shell and are driven over the reverse channel
//!    (`crate::jobs::JobHandle`), and the reverse channel exposes only a per-id
//!    `__job_cancel`, not a "cancel all plugin jobs" broadcast. There is therefore
//!    nothing in *this* process to cancel, and no faithful reverse call to make.
//!    Omitted deliberately. **INTEGRATE NOTE**: if cancelling shell-side plugin
//!    jobs on reload is required, add a `__job_cancel_by_plugin` reverse method and
//!    call it here — flagged, not invented.
//! 2. **No final `on_tab_switch` for the active tab.** The shell fires one extra
//!    `on_tab_switch` for `state.active_tab_id` so plugins keyed on the *last*
//!    event land on the focused tab (`list_open()` order is non-deterministic).
//!    corvus-be has no shell-pushed active-tab id (the security handler likewise
//!    derives "active" from a path passed in, not from stored state). We still
//!    fire `on_repo_open` for every open tab — which sets `__arbor_current_repo__`
//!    on each plugin — but cannot single out the focused tab as the last fire.
//!    **INTEGRATE NOTE**: the Phase-2 `set_active_tab` handler is the canonical
//!    place the active tab becomes known to this host; once it lands, a focused
//!    repo is re-asserted on the next `set_active_tab` from the FE after reload.

use arbor_plugin_core::prelude::PluginHost;
use corvus_core::prelude::CorvusState;
use serde_json::{json, Value};

/// Lock the shared plugin host mutably (same static `plugin_lifecycle` and the
/// read `plugin_introspect::with_host` use), mapping a poisoned/absent lock onto
/// the established error-string shape.
fn with_host_mut<R>(
    f: impl FnOnce(&mut PluginHost) -> Result<R, String>,
) -> Result<R, String> {
    let host = crate::plugin_introspect::host();
    let mut guard = host
        .lock()
        .map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&mut guard)
}

/// Rebuild the plugin runtime from disk and re-announce it — the body shared by
/// `reload_plugins` and the "enable" branch of `set_plugins_enabled`. Mirrors the
/// shell's `platform::plugin::reload_runtime` (minus the job-cancel + active-tab
/// fire that corvus-be can't faithfully reproduce — see module docs).
///
/// Order matters and matches the shell: rebuild the host (which fires
/// `on_plugin_unload`/`on_plugin_load` internally) and restart its schedulers
/// **under the lock**, *release the lock*, then fire the per-tab lifecycle hooks
/// and emit — firing hooks while still holding the host lock would deadlock (the
/// broker dispatches into the host's own VMs).
fn reload_runtime(ctx: &CorvusState) -> Result<(), String> {
    // Rebuild the host + restart schedulers, then drop the guard before firing
    // any hooks (the broker re-enters the host).
    with_host_mut(|host| {
        host.reload().map_err(|e| e.to_string())?;
        host.start_all_schedulers();
        Ok(())
    })?;

    // Re-fire `on_repo_open` for every open tab so plugins that derive
    // `current_repo` from the last lifecycle event re-acquire it on the freshly
    // rebuilt VMs (the broadcast path also refreshes `__arbor_current_repo__`).
    // Resolve a display name from the repo registry (file-backed, reload-on-read),
    // falling back to the path's basename when the registry has no entry — the
    // same name the shell's `list_open()` would carry.
    let opens = ctx.open_tabs(); // Vec<(tab_id, repo_path)>
    if !opens.is_empty() {
        let names: Vec<(String, String, String)> = {
            let reg = crate::workspace::registry::registry(ctx);
            opens
                .into_iter()
                .map(|(tab_id, path)| {
                    let name = reg
                        .find_by_path(&path)
                        .map(|e| e.display_name.clone())
                        .unwrap_or_else(|| basename(&path));
                    (tab_id, path, name)
                })
                .collect()
        }; // drop the registry guard before firing hooks
        for (tab_id, path, name) in &names {
            ctx.fire_hook(
                "on_repo_open",
                json!({ "tab_id": tab_id, "path": path, "name": name }),
            );
        }
    }

    ctx.emit("arbor://plugins-reloaded", Value::Null);
    Ok(())
}

/// Last path segment as a fallback repo name (registry miss) — mirrors how the
/// shell derives an open-tab name from its path when no nicer label exists.
fn basename(path: &str) -> String {
    path.replace('\\', "/")
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

/// Reload all plugins from disk: rebuild the host, restart schedulers, re-fire
/// `on_repo_open` for open tabs, and broadcast `arbor://plugins-reloaded`.
/// Mirrors the shell's `platform::plugin::reload_plugins`.
#[arbor_rpc::handler]
fn reload_plugins(ctx: &CorvusState) -> Result<(), String> {
    reload_runtime(ctx)
}

/// Master plugin-system kill-switch. Persists the choice, then either reloads the
/// runtime from disk (`enabled`) or tears it down (`!enabled`). Mirrors the
/// shell's `platform::plugin::set_plugins_enabled`.
///
/// **Persistence:** the shell saves `plugins_enabled` into its `AppConfig`
/// (`~/.config/arbor/config.toml`) *before* mutating the runtime, and short-
/// circuits when the flag is already at the requested value. corvus-be does not
/// own that config — the typed `AppConfig` + its TOML writer live in the shell —
/// so the kill-switch flag is persisted over the reverse channel via the
/// `__set_plugins_enabled` host method (the shell's matching handler does the
/// idempotent compare-and-save, returning whether a change was applied). When the
/// stored value already matched, the shell signals "unchanged" and we skip the
/// runtime mutation, exactly as the shell's early `return Ok(())`.
///
/// **INTEGRATE NOTE — reverse method required.** This handler calls
/// `ctx.host_call("__set_plugins_enabled", { enabled })`, expecting the shell to
/// expose that method (read `cfg.plugins_enabled`; if equal → return `false`;
/// else set it, `config::app_config::save`, return `true`). If the integrate
/// phase prefers the flag stay shell-local (the FE could persist via the existing
/// `platform` `set_plugins_enabled` for the *config* write and call corvus-be only
/// for the *runtime* mutation), drop the `host_call` and have the FE split the
/// two — flagged so the wiring decision is explicit, not silently assumed.
#[arbor_rpc::handler]
fn set_plugins_enabled(ctx: &CorvusState, enabled: bool) -> Result<(), String> {
    // Persist the choice (idempotent compare-and-save) before touching the
    // runtime, so a crash in between can't desync the saved flag from what ran.
    // The shell owns the typed config; the reverse method does the compare-save
    // and reports whether anything changed.
    let changed: bool = match ctx.host_call("__set_plugins_enabled", json!({ "enabled": enabled })) {
        Ok(v) => serde_json::from_value(v).unwrap_or(true),
        Err(e) => {
            // Mirror the shell's behaviour of treating a persist failure as
            // non-fatal (`tracing::warn!` + proceed): warn, assume the flag
            // changed, and apply the runtime mutation anyway.
            eprintln!("corvus-be: failed to persist plugins_enabled: {e}");
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
        // Tear the runtime down. (No local job registry to cancel first — see the
        // module-level "No plugin-job cancellation" note.) `unload_all` fires
        // `on_plugin_unload` and drops contributions/tree/icons internally.
        with_host_mut(|host| {
            host.unload_all();
            Ok(())
        })?;
        ctx.emit("arbor://plugins-reloaded", Value::Null);
    }
    Ok(())
}
