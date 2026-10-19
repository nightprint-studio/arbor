//! Generic Model-D IPC entry point (M3 Asse B).
//!
//! The whole point of the Model-D seam is that the shell is a *router*, not a
//! place that re-declares every product's command signatures. So instead of one
//! `#[tauri::command]` per command, the FE forwards everything through this
//! single command as `(program, method, params)`; the shell dispatches it to
//! the right product backend over `arbor-ipc`. The command names, argument
//! shapes and return types live once, on the backend (see
//! `crate::ipc::corvus`), and the FE keeps its typed wrappers for ergonomics.

use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::AppState;

/// Forward a product command to its backend.
///
/// - `program` — which backend (`"corvus"` / `"platform"` today; `"merula"` /
///   `"sitta"` later).
/// - `method` — the method the backend registered (= the handler's fn name,
///   e.g. `"stash_save"`).
/// - `params` — the handler's named arguments as a JSON object (snake_case keys
///   matching the handler signature); omitted/`null` for no-arg methods.
///
/// The result is the handler's return value as JSON. Backend errors arrive with
/// their original wire string preserved (see `crate::ipc::dispatch_rpc`).
///
/// `async` + a single central `spawn_blocking`: every sync handler reached
/// through the router runs on the blocking pool — off the main thread (no UI
/// freeze) and off the runtime workers — which is exactly the `spawn_blocking`
/// each heavy inline git command used to do for itself. Handlers therefore stay
/// plain sync functions; a handler that needs to keep concurrency just
/// brief-locks `repos` to clone the path, drops the lock, then does its heavy
/// work on a reopened repo (same shape as the old commands, minus the wrapper).
#[tauri::command]
pub async fn rpc(
    app: AppHandle,
    program: String,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    let params = params.unwrap_or(serde_json::Value::Null);

    // Async handlers (network/credential) are awaited on the runtime — no
    // blocking-pool thread is held for the round-trip. They fire their own
    // fire-and-forget hooks inline (host co-located), so no post-hooks are owed.
    //
    // Exception: when a product backend advertises this method as served
    // out-of-process, we must NOT short-circuit to the in-process path — the
    // call has to flow through the router/`SplitBroker` to that process. The
    // blocking-pool path below does exactly that (it waits on the child round-
    // trip off the runtime workers). Without this guard, every async method —
    // e.g. the issue-tracker handlers `corvus-be` now serves — would stay pinned
    // in-process, defeating the split.
    if crate::ipc::is_async_method(&program, &method)
        && !crate::ipc::is_oop_method(&program, &method)
    {
        return crate::ipc::dispatch_async(&app, &program, &method, params).await;
    }

    // Sync handlers (CPU-bound git via libgit2) run on `spawn_blocking`, off the
    // runtime workers — the original path, unchanged.
    tokio::task::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = crate::ipc::dispatch_rpc(state.inner(), &program, &method, params.clone())?;
        // The corvus handlers fire their own fire-and-forget plugin hooks inline
        // (the plugin host is co-located with them). The platform backend's
        // launcher-level hooks still fire from a post-hooks table here until they
        // move to the launcher broadcast channel; its `program` guard makes this
        // a no-op for non-platform calls.
        crate::ipc::platform::post_hooks::fire(state.inner(), &program, &method, &params, &result);
        Ok(result)
    })
    .await
    .map_err(|e| AppError::Other(format!("rpc task panicked: {e}")))?
}

/// Bring a product backend up for a caller that is not that product's own window.
///
/// `rpc` only reaches a backend that is already running; a product's window starts its own on
/// open. A different product that wants to borrow one (Bennu's blame gutter asking Corvus) has
/// no window to do that, so it asks here first. Idempotent — an attached backend is a no-op —
/// and run on the blocking pool because `ensure_*_be` parks on the child's first frame, which
/// on a runtime worker is the deadlock `docs/backend-architecture.md` warns about.
///
/// Only products meant to be borrowed are listed; this is not a way to spawn arbitrary
/// programs.
#[tauri::command]
pub async fn ensure_product_backend(app: AppHandle, program: String) -> Result<(), AppError> {
    let ensure: fn(&AppHandle) = match program.as_str() {
        "corvus" => crate::ipc::ensure_corvus_be,
        "picus" => crate::ipc::ensure_picus_be,
        other => return Err(AppError::Other(format!("'{other}' is not a borrowable backend"))),
    };
    tokio::task::spawn_blocking(move || ensure(&app))
        .await
        .map_err(|e| AppError::Other(format!("ensure backend task panicked: {e}")))
}
