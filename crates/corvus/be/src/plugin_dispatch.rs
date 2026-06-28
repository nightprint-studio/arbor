//! Plugin-runtime **dispatch** ops over the OOP boundary — the write surface
//! that fires hooks / Lua handlers **into** corvus-be's loaded plugin VMs.
//!
//! After the Phase-2 flip the shell loads no Corvus product plugins; the live
//! Lua VMs (and the `ContributionRegistry`) live here, in `corvus-be`. The
//! Plugin Manager / declarative-UI dispatch ops therefore have to run here too,
//! or they'd fire into the now-empty shell host and no-op. This module re-serves
//! them as `corvus`-program RPC handlers operating on the same module-static
//! `Arc<Mutex<PluginHost>>` that `main` publishes via
//! [`crate::plugin_introspect::install`] (read through
//! [`crate::plugin_introspect::host`] — one host, one source of truth).
//!
//! Ported here (mirrors `src-tauri/src/ipc/platform/plugin.rs` byte-for-byte on
//! the parts that touch the host):
//!   * `exec_hook`          — manually fire a named hook with a JSON payload.
//!   * `fire_plugin_action` — deliver an `on:<action>` to one plugin's VM.
//!   * `fire_command`       — invoke a registered command on behalf of a caller.
//!   * `set_active_tab`     — fire `on_tab_switch` so the broadcast path refreshes
//!     the per-plugin `__arbor_current_repo__` global (the active-repo notion the
//!     ported `arbor.repo.*` namespaces read when called outside a hook).
//!
//! These dispatched actions may themselves call the `ns_shell` namespaces (now
//! available in corvus-be) — that round-trip back through the same host is the
//! whole point of co-locating them here.

use arbor_plugin_core::prelude::{PluginHost, fire_on};
use corvus_core::prelude::CorvusState;

/// Lock the shared host (write side). Same static the read handlers in
/// `plugin_introspect` lock via `with_host` — borrowed through that module's
/// `host()` accessor so there is exactly one `Arc<Mutex<PluginHost>>`.
fn with_host<R>(f: impl FnOnce(&PluginHost) -> Result<R, String>) -> Result<R, String> {
    let host = crate::plugin_introspect::host();
    let guard = host
        .lock()
        .map_err(|_| "plugin host mutex poisoned".to_string())?;
    f(&guard)
}

/// Manually fire a named hook with a JSON-string payload onto every subscriber.
/// Faithful to the shell's `exec_hook`: a malformed `context_json` degrades to
/// an empty object, and the fire goes through the broker bound to this host
/// (`CorvusState::fire_hook`), which is a no-op when nothing subscribes.
#[arbor_rpc::handler]
fn exec_hook(ctx: &CorvusState, hook: String, context_json: String) -> Result<(), String> {
    let payload: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    ctx.fire_hook(&hook, payload);
    Ok(())
}

/// Fire a specific action on a specific plugin (declarative UI element click).
/// Mirrors the shell's `fire_plugin_action`: targeted delivery to the one
/// plugin's VM via `fire_on` (no-op when that plugin is missing/disabled).
#[arbor_rpc::handler]
fn fire_plugin_action(
    _ctx: &CorvusState,
    plugin_name: String,
    action: String,
    context_json: String,
) -> Result<(), String> {
    with_host(|host| {
        fire_on(host, &plugin_name, &action, &context_json);
        Ok(())
    })
}

/// Invoke a registered command on behalf of `caller_plugin` (the declarative
/// `kind = "command"` dispatch path; capability gates live in the host).
/// Mirrors the shell: `args` (when present and non-null) is merged into the
/// context object under `"args"`, and the host's `CommandError` is rendered
/// `"{kind}: {message}"` — byte-identical to the shell's
/// `format!("{}: {}", e.kind(), e.message())`.
#[arbor_rpc::handler]
fn fire_command(
    _ctx: &CorvusState,
    caller_plugin: String,
    id: String,
    args: Option<serde_json::Value>,
    context_json: String,
) -> Result<(), String> {
    let mut command_ctx: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    if let Some(a) = args {
        if !a.is_null() {
            if let Some(obj) = command_ctx.as_object_mut() {
                obj.insert("args".to_string(), a);
            }
        }
    }
    with_host(|host| {
        host.invoke_command(&caller_plugin, &id, &command_ctx)
            .map_err(|e| format!("{}: {}", e.kind(), e.message()))
    })
}

/// Inform the backend which tab is currently active. Fires the `on_tab_switch`
/// hook, whose broadcast path (`hook_router::fire_broadcast`) refreshes the
/// per-plugin `__arbor_current_repo__` global on every loaded VM — corvus-be's
/// active-repo notion, read by the ported `arbor.repo.*` namespaces when a
/// command runs outside a hook.
///
/// Faithful to the shell's `set_active_tab`: only fires when a real tab is
/// activated (`Some`), and resolves the repo path via the shell-pushed
/// open-repos map (`CorvusState::repo_path`, keyed by `tab_id` like the shell's
/// `RepoManager`). The shell never errored on an unregistered tab — it fired
/// with an empty path/name — so we do the same (empty `path` → the broadcast
/// leaves the global untouched, matching the shell). corvus-be keeps no per-tab
/// repo *name*, so it is derived from the path's basename (informational only;
/// the active-repo resolution keys solely on `path`).
#[arbor_rpc::handler]
fn set_active_tab(ctx: &CorvusState, tab_id: Option<String>) -> Result<(), String> {
    if let Some(tid) = tab_id {
        let path = ctx.repo_path(&tid).unwrap_or_default();
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        ctx.fire_hook(
            "on_tab_switch",
            serde_json::json!({
                "tab_id": tid,
                "path":   path,
                "name":   name,
            }),
        );
    }
    Ok(())
}
