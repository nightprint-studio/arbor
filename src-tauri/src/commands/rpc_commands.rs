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
    if crate::ipc::is_async_method(&program, &method) {
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
