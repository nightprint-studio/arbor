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
pub fn fire(state: &AppState, program: &str, method: &str, params: &Value, _result: &Value) {
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
        _ => {}
    }
}
