//! Routing-independent post-call plugin hooks for the `corvus` backend.
//!
//! A few git operations fire a fire-and-forget plugin hook *after* they
//! succeed (`on_stash_push`, `on_stash_pop`). When the method runs in-process
//! the handler could fire it inline — but once the method is served
//! out-of-process by `corvus-be`, the in-process handler is bypassed entirely,
//! so an inline fire would silently stop happening. The hook firing therefore
//! lives **here**, in the shell's generic `rpc` path, where it runs exactly
//! once regardless of whether the method was served in-process (loopback) or
//! out-of-process. The handlers fire no hooks themselves.
//!
//! Only fire-and-forget hooks belong here. Vetoable hooks (`on_pre_commit`)
//! need a different seam (a round-trip the backend awaits) and live in domains
//! not yet moved.
//!
//! Payload note: the stash entry's `index`/`message` come from the **result**
//! (git rewrites the message to "WIP on …" when the user gave none), while
//! `tab_id`/`include_untracked`/`index`-to-pop come from the **params** — same
//! split the inline handlers used.

use serde_json::{json, Value};

use crate::AppState;

/// Fire any plugin hook owed by a successful `(program, method)` call.
///
/// `params` is the handler's JSON arguments; `result` its JSON return value.
/// Called from the `rpc` command after a successful `dispatch_rpc`.
pub fn fire(state: &AppState, program: &str, method: &str, params: &Value, result: &Value) {
    if program != "corvus" {
        return;
    }
    match method {
        "stash_save" => {
            state.fire_hook(
                "on_stash_push",
                json!({
                    "tab_id":            params.get("tab_id"),
                    "index":             result.get("index"),
                    "message":           result.get("message"),
                    "include_untracked": params.get("include_untracked"),
                }),
            );
        }
        // Clean apply (no conflicts) → an `on_stash_pop` with `drop:false`.
        "stash_apply" if clean(result) => {
            state.fire_hook(
                "on_stash_pop",
                json!({ "tab_id": params.get("tab_id"), "index": params.get("index"), "drop": false }),
            );
        }
        // Clean pop (no conflicts) → an `on_stash_pop` with `drop:true`. A
        // conflicted pop leaves the stash present, so no hook (as before).
        "stash_pop" if clean(result) => {
            state.fire_hook(
                "on_stash_pop",
                json!({ "tab_id": params.get("tab_id"), "index": params.get("index"), "drop": true }),
            );
        }
        _ => {}
    }
}

/// A `StashApplyResult` with no conflicts. Missing/!bool → treated as conflicted
/// (no hook), which is the safe default.
fn clean(result: &Value) -> bool {
    result.get("has_conflicts").and_then(Value::as_bool) == Some(false)
}
