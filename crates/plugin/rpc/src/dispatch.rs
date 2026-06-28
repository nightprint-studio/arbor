//! Runtime **dispatch** into the loaded plugin VMs — fire hooks / Lua handlers /
//! commands, and announce the active tab. Generic port of `corvus-be`'s former
//! `plugin_dispatch.rs`.
//!
//! These actions may themselves call the relocated git `arbor.*` namespaces (now
//! running in the same backend) — that round-trip through the same host is the
//! whole point of co-locating dispatch with the host.

use arbor_plugin_core::prelude::fire_on;

use crate::context::{with_host, PluginRpcContext};

/// Manually fire a named hook with a JSON-string payload onto every subscriber.
/// A malformed `context_json` degrades to an empty object; the fire is a no-op
/// when nothing subscribes.
pub fn exec_hook<C: PluginRpcContext>(
    ctx: &C,
    hook: String,
    context_json: String,
) -> Result<(), String> {
    let payload: serde_json::Value =
        serde_json::from_str(&context_json).unwrap_or_else(|_| serde_json::json!({}));
    ctx.fire_hook(&hook, payload);
    Ok(())
}

/// Fire a specific action on a specific plugin (declarative-UI element click):
/// targeted delivery to the one plugin's VM via `fire_on` (no-op when that plugin
/// is missing/disabled).
pub fn fire_plugin_action<C: PluginRpcContext>(
    ctx: &C,
    plugin_name: String,
    action: String,
    context_json: String,
) -> Result<(), String> {
    with_host(ctx, |host| {
        fire_on(host, &plugin_name, &action, &context_json);
        Ok(())
    })
}

/// Invoke a registered command on behalf of `caller_plugin`. `args` (when present
/// and non-null) is merged into the context object under `"args"`; the host's
/// `CommandError` renders `"{kind}: {message}"`.
pub fn fire_command<C: PluginRpcContext>(
    ctx: &C,
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
    with_host(ctx, |host| {
        host.invoke_command(&caller_plugin, &id, &command_ctx)
            .map_err(|e| format!("{}: {}", e.kind(), e.message()))
    })
}

/// Inform the backend which tab is active. Fires `on_tab_switch`, whose broadcast
/// path refreshes the per-plugin `__arbor_current_repo__` global on every loaded
/// VM. Only fires when a real tab is activated (`Some`); an unregistered tab
/// resolves to an empty path (the broadcast then leaves the global untouched),
/// matching the shell. The name is informational (basename of the path); the
/// active-repo resolution keys solely on `path`.
pub fn set_active_tab<C: PluginRpcContext>(
    ctx: &C,
    tab_id: Option<String>,
) -> Result<(), String> {
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
