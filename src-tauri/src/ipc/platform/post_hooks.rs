//! Routing-independent post-call plugin hooks for the `platform` backend.
//!
//! A few platform commands owe a fire-and-forget plugin hook *after* they
//! succeed. The hook fires here, in the shell's generic `rpc` path, so it runs
//! exactly once whether the method was served in-process or (eventually)
//! out-of-process — the handlers fire none themselves.
//!
//! These are **launcher-level** hooks (`on_theme_changed`, `on_workspace_*`)
//! whose payload is reconstructable from `(params, result)`. They are the
//! interim home until they move to the launcher broadcast channel
//! (`docs/plugin-relocation-inventory.md`, Wave 2). The corvus domain, by
//! contrast, now fires its hooks inline from the handlers (host co-located).

use serde_json::{json, Value};

use crate::AppState;

/// Fire any plugin hook owed by a successful `(program, method)` call on the
/// platform backend. Called from the `rpc` command after a successful dispatch.
pub fn fire(state: &AppState, program: &str, method: &str, params: &Value, _result: &Value) {
    if program != "platform" {
        return;
    }
    // The theme switch broadcasts to plugins. Payload mirrors the original
    // inline `notify_theme_changed` fire (all fields from params).
    //
    // The workspace hooks (`on_workspace_created`/`_updated`/`_repo_added`/
    // `_repo_removed`) moved with the handlers to corvus-be (ADR-1), where the
    // co-located plugin host fires them inline. They are no longer owed here.
    if method == "notify_theme_changed" {
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
}
