//! Host built-in command handlers (`arbor:area.verb`).
//!
//! A plugin holding the `command_invoke` permission can invoke a host command
//! declaratively (`dispatch = { kind = "command", id = "arbor:git.commit" }`)
//! or at runtime (`arbor.command.fire("arbor:git.commit", ctx)`). Resolution +
//! both capability gates run in `arbor_plugin_core` (`PluginHost::invoke_command`
//! plus `host_command_required`); a gated invocation reaches this module through
//! `TauriAppCtx::invoke_host_command`.
//!
//! Handlers reuse the existing `#[tauri::command]` functions verbatim — no git
//! logic is duplicated here. We only parse the plugin-supplied context into the
//! command's parameters and forward the call (so the command's own hook firing
//! / event emission happens exactly as for a user-initiated invocation).
//!
//! The allowlist of ids + the permission tier each requires lives in
//! `arbor_plugin_core::prelude::host_command_required`. Keep the `match` below
//! in lockstep with that table: a built-in gated there but unmatched here is a
//! silent no-op (logged), and vice-versa it is unreachable.

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::AppState;

/// Dispatch a gated host built-in command. Runs on the host async runtime
/// (spawned by `TauriAppCtx::invoke_host_command`), so the plugin-host lock the
/// caller held is already released and the handlers' hook firing can't deadlock.
pub async fn dispatch(app: &AppHandle, id: &str, ctx_json: &str) -> Result<(), AppError> {
    let ctx: Value = serde_json::from_str(ctx_json).unwrap_or_else(|_| Value::Object(Default::default()));

    // ── Frontend intents (no AppState, no permission tier) ───────────────────
    // The shell only relays the verb; the UI executes it (see the
    // `arbor://host-ui-command` listener in AppShell.svelte).
    if matches!(id, "arbor:repo.refresh" | "arbor:app.open_settings") {
        let verb = id.strip_prefix("arbor:").unwrap_or(id);
        let _ = app.emit("arbor://host-ui-command", serde_json::json!({ "id": verb }));
        return Ok(());
    }

    // ── Backend (git) commands ───────────────────────────────────────────────
    let state = app.state::<AppState>();
    let tab_id = resolve_tab_id(&state, &ctx)
        .ok_or_else(|| AppError::Other(format!("{id}: no tab_id supplied and no active tab")))?;

    match id {
        "arbor:git.commit" => {
            let message = req_str(&ctx, "message", id)?;
            let amend = get_bool(&ctx, "amend").unwrap_or(false);
            // Migrated to the corvus broker: route through the generic rpc path.
            // The handler fires `on_pre_commit` (vetoable) + `on_commit` inline,
            // so a plugin veto surfaces here as an Err exactly as before.
            corvus_rpc(state.inner(), "commit",
                serde_json::json!({ "tab_id": tab_id, "message": message, "amend": amend }))?;
        }
        "arbor:git.push" => {
            let remote = get_str(&ctx, "remote").unwrap_or_else(|| "origin".to_string());
            let refspec = req_str(&ctx, "refspec", id)?;
            let force = get_bool(&ctx, "force").unwrap_or(false);
            corvus_rpc_async(app, "push_branch", serde_json::json!({
                "tab_id": tab_id, "remote": remote, "refspec": refspec, "force": force,
            })).await?;
        }
        "arbor:git.fetch" => {
            let remote = get_str(&ctx, "remote").unwrap_or_else(|| "origin".to_string());
            corvus_rpc_async(app, "fetch_remote", serde_json::json!({
                "tab_id": tab_id, "remote": remote,
            })).await?;
        }
        "arbor:git.pull" => {
            let remote = get_str(&ctx, "remote").unwrap_or_else(|| "origin".to_string());
            corvus_rpc_async(app, "pull_branch", serde_json::json!({
                "tab_id": tab_id, "remote": remote, "op_id": serde_json::Value::Null,
            })).await?;
        }
        "arbor:git.branch_create" => {
            let name = req_str(&ctx, "name", id)?;
            let from_oid = get_str(&ctx, "from_oid").unwrap_or_else(|| "HEAD".to_string());
            // Migrated to the corvus broker: route through the generic rpc path
            // so `on_branch_create` fires exactly as for a user invocation.
            corvus_rpc(state.inner(), "create_branch",
                serde_json::json!({ "tab_id": tab_id, "name": name, "from_oid": from_oid }))?;
        }
        "arbor:git.checkout" => {
            let name = req_str(&ctx, "name", id)?;
            // Migrated to the corvus broker: route through the generic rpc path
            // so `on_checkout` + worktree-link sync fire as for a user invocation.
            corvus_rpc(state.inner(), "checkout_branch",
                serde_json::json!({ "tab_id": tab_id, "name": name }))?;
        }
        "arbor:git.branch_delete" => {
            let name = req_str(&ctx, "name", id)?;
            corvus_rpc(state.inner(), "delete_branch",
                serde_json::json!({ "tab_id": tab_id, "name": name }))?;
        }
        "arbor:git.stage_all" => {
            corvus_rpc(state.inner(), "stage_all", serde_json::json!({ "tab_id": tab_id }))?;
        }
        "arbor:git.unstage_all" => {
            corvus_rpc(state.inner(), "unstage_all", serde_json::json!({ "tab_id": tab_id }))?;
        }
        other => {
            return Err(AppError::Other(format!(
                "host command '{other}' is gated but has no handler (allowlist / dispatch drift)"
            )));
        }
    }
    Ok(())
}

/// Forward a now-migrated git command through the generic `corvus` rpc path.
/// The corvus handler fires its own fire-and-forget plugin hooks **inline**
/// (the plugin host is co-located with the handler), so nothing is owed here
/// beyond dispatch — a plugin veto on `commit` surfaces as the dispatch `Err`.
fn corvus_rpc(state: &AppState, method: &str, params: Value) -> Result<(), AppError> {
    crate::ipc::dispatch_rpc(state, "corvus", method, params)?;
    Ok(())
}

/// Async sibling of [`corvus_rpc`] for the network-coupled corvus handlers
/// (`fetch_remote` / `push_branch` / `pull_branch`), which register as `async`
/// and are awaited on the runtime via [`crate::ipc::dispatch_async`] rather than
/// the sync blocking-pool path. Same contract: the handler fires its own hooks
/// inline, so nothing is owed here beyond dispatch.
async fn corvus_rpc_async(app: &AppHandle, method: &str, params: Value) -> Result<(), AppError> {
    crate::ipc::dispatch_async(app, "corvus", method, params).await?;
    Ok(())
}

/// Resolve which repo the command targets: an explicit `tab_id` in the plugin
/// context wins; otherwise fall back to the active tab.
fn resolve_tab_id(state: &AppState, ctx: &Value) -> Option<String> {
    if let Some(t) = get_str(ctx, "tab_id") {
        return Some(t);
    }
    state.active_tab_id.lock().ok().and_then(|g| g.clone())
}

/// Read a parameter, checking the declared static `args` first (button
/// dispatch) then the top-level context (`arbor.command.fire(id, { … })`).
fn field<'a>(ctx: &'a Value, key: &str) -> Option<&'a Value> {
    ctx.get("args")
        .and_then(|a| a.get(key))
        .or_else(|| ctx.get(key))
}

fn get_str(ctx: &Value, key: &str) -> Option<String> {
    field(ctx, key).and_then(|v| v.as_str()).map(str::to_string)
}

fn get_bool(ctx: &Value, key: &str) -> Option<bool> {
    field(ctx, key).and_then(Value::as_bool)
}

fn req_str(ctx: &Value, key: &str, id: &str) -> Result<String, AppError> {
    get_str(ctx, key)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Other(format!("{id}: '{key}' is required")))
}
