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

        // ── gitflow ── all fire-and-forget. *_start carry the resolved
        // base_branch from the result; *_finish omit it (matches the original
        // inline payloads). Both init methods fire on_flow_init with {tab_id}.
        "gitflow_init" | "gitflow_init_create_main" => {
            state.fire_hook("on_flow_init", json!({ "tab_id": params.get("tab_id") }));
        }
        "gitflow_feature_start" => {
            state.fire_hook("on_flow_feature_start", json!({
                "tab_id": params.get("tab_id"),
                "name": params.get("name"),
                "base_branch": result.get("base_branch"),
            }));
        }
        "gitflow_feature_finish" => {
            state.fire_hook("on_flow_feature_finish", json!({
                "tab_id": params.get("tab_id"), "name": params.get("name"),
            }));
        }
        "gitflow_release_start" => {
            state.fire_hook("on_flow_release_start", json!({
                "tab_id": params.get("tab_id"),
                "version": params.get("version"),
                "base_branch": result.get("base_branch"),
            }));
        }
        "gitflow_release_finish" => {
            state.fire_hook("on_flow_release_finish", json!({
                "tab_id": params.get("tab_id"), "version": params.get("version"),
            }));
        }
        "gitflow_hotfix_start" => {
            state.fire_hook("on_flow_hotfix_start", json!({
                "tab_id": params.get("tab_id"),
                "name": params.get("name"),
                "base_branch": result.get("base_branch"),
            }));
        }
        "gitflow_hotfix_finish" => {
            state.fire_hook("on_flow_hotfix_finish", json!({
                "tab_id": params.get("tab_id"), "name": params.get("name"),
            }));
        }

        // ── rebase ── fire-and-forget. on_rebase_start's action_count comes
        // from the request `todo` length (the handler returns () with no count).
        "start_rebase" => {
            let action_count = params.get("todo").and_then(Value::as_array).map(|a| a.len());
            state.fire_hook("on_rebase_start", json!({
                "tab_id": params.get("tab_id"),
                "base": params.get("base"),
                "action_count": action_count,
            }));
        }
        "rebase_abort" => {
            state.fire_hook("on_rebase_abort", json!({ "tab_id": params.get("tab_id") }));
        }

        // ── branch ── fire-and-forget. The migrated handlers fire no hooks;
        // payloads mirror the original inline fires (P = from params, R = from
        // result).
        "create_branch" => {
            state.fire_hook("on_branch_create", json!({
                "tab_id":   params.get("tab_id"),    // P
                "name":     params.get("name"),      // P
                "from_oid": params.get("from_oid"),  // P
            }));
        }
        // `delete_branches` returns the Vec<String> of deleted local names.
        "delete_branches" => {
            if result.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                state.fire_hook("on_branch_delete", json!({
                    "tab_id": params.get("tab_id"),  // P
                    "names":  result,                // R (deleted names)
                }));
            }
        }
        // `delete_remote_branches` returns the FAILED names; the original inline
        // fired with the *deleted* set (requested names − failed).
        "delete_remote_branches" => {
            let names  = params.get("names").and_then(Value::as_array).cloned().unwrap_or_default();
            let failed = result.as_array().cloned().unwrap_or_default();
            let deleted: Vec<&Value> = names.iter().filter(|n| !failed.contains(n)).collect();
            if !deleted.is_empty() {
                state.fire_hook("on_branch_delete", json!({
                    "tab_id": params.get("tab_id"),  // P
                    "names":  deleted,               // P − R
                }));
            }
        }
        "rename_remote_branch" => {
            state.fire_hook("on_branch_rename", json!({
                "tab_id":        params.get("tab_id"),         // P
                "old_name":      params.get("old_full_name"),  // P
                "new_name":      result.get("new_full_name"),  // R
                "local_renamed": result.get("local_renamed"),  // R
            }));
        }
        "checkout_commit" => {
            state.fire_hook("on_checkout", json!({
                "tab_id": params.get("tab_id"),  // P
                "oid":    params.get("oid"),     // P
            }));
        }
        // Safe (auto-stash) checkout: only fire when the working tree landed
        // clean (no stash-apply error, no stash conflicts) — same gate the
        // original inline fire used.
        "checkout_commit_safe" if clean_checkout(result) => {
            state.fire_hook("on_checkout", json!({
                "tab_id": params.get("tab_id"),  // P
                "oid":    params.get("oid"),     // P
            }));
        }

        // ── missing project ── `report_repo_missing` returns the resolved
        // display name (Option<String>, JSON null when unresolved); the original
        // always fired regardless.
        "report_repo_missing" => {
            state.fire_hook("on_project_missing", json!({
                "repo_id": params.get("repo_id"),  // P
                "path":    params.get("path"),     // P
                "name":    result,                 // R (Option<String>)
                "reason":  params.get("reason"),   // P
            }));
        }
        // `relocate_repo` keeps emitting `arbor://repo-relocated` from the handler
        // (best-effort); only the plugin hook moves here. Fire only on an actual
        // move — the same-folder no-op leaves `name`/`old_path` null/equal.
        "relocate_repo" if result.get("name").map(|v| !v.is_null()).unwrap_or(false) => {
            state.fire_hook("on_project_relocated", json!({
                "repo_id":    params.get("repo_id"),     // P
                "old_path":   result.get("old_path"),    // R
                "new_path":   params.get("new_path"),    // P
                "name":       result.get("name"),        // R
                "remote_url": result.get("remote_url"),  // R
            }));
        }

        // ── repo open ── payload fully from params + the returned RepoInfo.
        "open_repo" => {
            state.fire_hook("on_repo_open", json!({
                "tab_id": params.get("tab_id"),  // P
                "path":   result.get("path"),    // R
                "name":   result.get("name"),    // R
            }));
        }

        // ── linked worktree ── member add/remove, both fields from params.
        "add_worktree_link_member" => {
            state.fire_hook("on_worktree_link_member_added", json!({
                "link_id": params.get("link_id"),  // P
                "repo_id": params.get("repo_id"),  // P
            }));
        }
        "remove_worktree_link_member" => {
            state.fire_hook("on_worktree_link_member_removed", json!({
                "link_id": params.get("link_id"),  // P
                "repo_id": params.get("repo_id"),  // P
            }));
        }

        _ => {}
    }
}

/// A safe-checkout result that landed clean: no stash-apply error and no
/// remaining stash conflicts. Missing/!expected shapes → treated as not-clean
/// (no hook), the safe default.
fn clean_checkout(result: &Value) -> bool {
    result.get("stash_apply_error").map(Value::is_null).unwrap_or(false)
        && result
            .get("stash_conflicts")
            .and_then(Value::as_array)
            .map(|a| a.is_empty())
            .unwrap_or(false)
}

/// A `StashApplyResult` with no conflicts. Missing/!bool → treated as conflicted
/// (no hook), which is the safe default.
fn clean(result: &Value) -> bool {
    result.get("has_conflicts").and_then(Value::as_bool) == Some(false)
}
