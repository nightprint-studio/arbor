//! Generic Model-D IPC entry point (M3 Asse B).
//!
//! The whole point of the Model-D seam is that the shell is a *router*, not a
//! place that re-declares every product's command signatures. So instead of one
//! `#[tauri::command]` per command, the FE forwards everything through this
//! single command as `(program, method, params)`; the shell dispatches it to
//! the right product backend over `arbor-ipc`. The command names, argument
//! shapes and return types live once, on the backend (see
//! `crate::ipc::corvus`), and the FE keeps its typed wrappers for ergonomics.

use tauri::State;

use crate::error::AppError;
use crate::AppState;

/// Forward a product command to its backend.
///
/// - `program` — which backend (`"corvus"` today; `"merula"` / `"sitta"` later).
/// - `method` — the `"<domain>.<verb>"` the backend registered (e.g. `"stash.save"`).
/// - `params` — the handler's named arguments as a JSON object (snake_case keys
///   matching the handler signature); omitted/`null` for no-arg methods.
///
/// The result is the handler's return value as JSON. Backend errors arrive with
/// their original wire string preserved (see `crate::ipc::dispatch_rpc`).
#[tauri::command]
pub fn rpc(
    state: State<'_, AppState>,
    program: String,
    method: String,
    params: Option<serde_json::Value>,
) -> Result<serde_json::Value, AppError> {
    crate::ipc::dispatch_rpc(&state, &program, &method, params.unwrap_or(serde_json::Value::Null))
}
