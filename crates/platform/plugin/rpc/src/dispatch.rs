//! Runtime **dispatch** into the loaded plugin VMs — fire hooks / Lua handlers /
//! commands, and announce the active tab. Generic port of `corvus-be`'s former
//! `plugin_dispatch.rs`.
//!
//! These actions may themselves call the relocated git `arbor.*` namespaces (now
//! running in the same backend) — that round-trip through the same host is the
//! whole point of co-locating dispatch with the host.

use arbor_plugin_core::prelude::fire_on;
use arbor_plugin_types::prelude::hook_names;

use crate::context::{with_host, PluginRpcContext};

/// Re-invoke a per-call anonymous Lua closure parked in a plugin VM's
/// `__arbor_hooks__` under a backend-minted synthetic name. The product
/// backend's namespace stores the closure (identical mechanism to `job.on_done`'s
/// `__job_done_<id>__`) and re-enters it through this targeted fire whenever the
/// pushed event fires — the per-call twin of [`fire_plugin_action`].
///
/// Fire-and-forget: a missing/disabled plugin or a missing closure is a silent
/// no-op (`fire_on`'s own contract).
pub fn invoke_plugin_callback<C: PluginRpcContext>(
    ctx: &C,
    plugin_name: String,
    callback_id: String,
    context_json: String,
) -> Result<(), String> {
    with_host(ctx, |host| {
        fire_on(host, &plugin_name, &callback_id, &context_json);
        Ok(())
    })
}

/// Drop a parked per-call closure from a plugin VM's `__arbor_hooks__` so it does
/// not leak when its stream ends or is unwatched. Teardown twin of
/// [`invoke_plugin_callback`]. A missing plugin/closure simply returns (the
/// host's `remove_hook` reports `false`, which we treat as a no-op).
pub fn remove_plugin_callback<C: PluginRpcContext>(
    ctx: &C,
    plugin_name: String,
    callback_id: String,
) -> Result<(), String> {
    with_host(ctx, |host| {
        host.remove_hook(&plugin_name, &callback_id);
        Ok(())
    })
}

/// Invoke a registered pipeline op (`arbor.pipeline.register_op`) on a plugin VM
/// and return its normalised `PipelineOpResult` as JSON. Unlike the fire-and-forget
/// callbacks above this is **request/reply**: the pipeline orchestrator blocks on
/// the structured `{ exit_code, stdout, stderr }` result, so it must travel back.
/// `params_json` is parsed into the value handed to the Lua handler; a malformed
/// payload degrades to an empty object.
pub fn invoke_pipeline_op<C: PluginRpcContext>(
    ctx: &C,
    plugin_name: String,
    op: String,
    params_json: String,
    cwd: String,
) -> Result<serde_json::Value, String> {
    let params: serde_json::Value =
        serde_json::from_str(&params_json).unwrap_or_else(|_| serde_json::json!({}));
    with_host(ctx, |host| {
        let result = host.invoke_pipeline_op(&plugin_name, &op, &params, &cwd)?;
        Ok(serde_json::json!({
            "exit_code": result.exit_code,
            "stdout":    result.stdout,
            "stderr":    result.stderr,
        }))
    })
}

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
            hook_names::arbor::TAB_SWITCH,
            serde_json::json!({
                "tab_id": tid,
                "path":   path,
                "name":   name,
            }),
        );
    }
    Ok(())
}
