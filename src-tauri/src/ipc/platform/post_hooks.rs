//! Routing-independent post-call plugin hooks for the `platform` backend.
//!
//! Mirror of [`crate::ipc::corvus::post_hooks`] for the platform program: a few
//! platform commands owe a fire-and-forget plugin hook *after* they succeed.
//! The hook fires here, in the shell's generic `rpc` path, so it runs exactly
//! once whether the method was served in-process or (eventually) out-of-process
//! by `platform-be` — the handlers fire none themselves.

use serde_json::{json, Value};

use crate::AppState;

/// Fire any plugin hook owed by a successful `(program, method)` call on the
/// platform backend. Called from the `rpc` command after a successful dispatch.
pub fn fire(state: &AppState, program: &str, method: &str, params: &Value, result: &Value) {
    if program != "platform" {
        return;
    }
    match method {
        // The theme switch broadcasts to plugins. Payload mirrors the original
        // inline `notify_theme_changed` fire (all fields from params).
        "notify_theme_changed" => {
            state.fire_hook(
                "on_theme_changed",
                json!({
                    "theme_id":   params.get("theme_id"),
                    "theme_name": params.get("theme_name"),
                    "vars":       params.get("vars"),
                    "source":     params.get("source"),
                }),
            );
        }

        // ── workspaces ── fire-and-forget. The migrated handlers fire no hooks;
        // payloads mirror the original inline `workspace_payload(&ws)` shape
        // ({id, name, color_idx, repo_ids, group_id, repo_count}), rebuilt here
        // from the returned `WorkspaceDef` (R) so the bytes match.
        "create_workspace" => {
            state.fire_hook("on_workspace_created", workspace_payload(result));
        }
        "update_workspace" => {
            state.fire_hook("on_workspace_updated", workspace_payload(result));
        }

        // ── repo membership ── both payloads come from params (P).
        "add_repo_to_workspace" => {
            state.fire_hook(
                "on_workspace_repo_added",
                json!({
                    "workspace_id": params.get("workspace_id"),
                    "repo_id":      params.get("repo_id"),
                }),
            );
        }
        // A move is a remove from the source workspace followed by an add to the
        // target — same two hooks the original inline command fired, in order.
        "move_repo_between_workspaces" => {
            let repo_id = params.get("repo_id");
            state.fire_hook(
                "on_workspace_repo_removed",
                json!({
                    "workspace_id": params.get("from_workspace_id"),
                    "repo_id":      repo_id,
                }),
            );
            state.fire_hook(
                "on_workspace_repo_added",
                json!({
                    "workspace_id": params.get("to_workspace_id"),
                    "repo_id":      repo_id,
                }),
            );
        }

        _ => {}
    }
}

/// Rebuild the `workspace_payload` shape the inline workspace commands fired,
/// from a serialized `WorkspaceDef` (the handler's return value). Mirrors
/// `crate::commands::workspace_commands::workspace_payload`: the same field set,
/// plus `repo_count` derived from the `repo_ids` array length.
fn workspace_payload(ws: &Value) -> Value {
    let repo_count = ws
        .get("repo_ids")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    json!({
        "id":         ws.get("id"),
        "name":       ws.get("name"),
        "color_idx":  ws.get("color_idx"),
        "repo_ids":   ws.get("repo_ids"),
        "group_id":   ws.get("group_id"),
        "repo_count": repo_count,
    })
}
