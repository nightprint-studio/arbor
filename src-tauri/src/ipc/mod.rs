//! Model-D IPC wiring (M3 Asse B — in-process pilot).
//!
//! Shell side of the `arbor-ipc` seam: it builds the
//! [`Router`](arbor_shell_common::prelude::Router) and exposes the generic
//! [`dispatch_rpc`] used by the single `rpc` Tauri command
//! (`crate::commands::rpc_commands`).
//!
//! ```text
//!   FE invoke("rpc", {program, method, params})
//!        │
//!        ▼
//!   rpc command ─▶ dispatch_rpc ─▶ Router ─(BrokerClient)─▶ LoopbackBroker
//!                                                              │
//!                                                              ▼
//!                                          corvus::dispatch(method, json) ─▶ registry handler
//! ```
//!
//! The shell declares **no per-command signature** — it forwards `(program,
//! method, params)` opaquely. The signatures live once, on the product backend
//! (`corvus::stash`), reachable in-process today via a [`LoopbackBroker`] whose
//! dispatch closure captures the live `AppHandle`. When the backend splits out,
//! only the registered `BrokerClient` changes (to a pipe/socket `tarpc` client)
//! — the router, this module and the `rpc` command don't move.
//!
//! ## Scope (pilot)
//!
//! Only the **stash** domain is registered so far (see [`corvus::stash`]); it's
//! the vertical slice that proves the generic seam (reads, writes, hook-firing,
//! error mapping) before the wider sweep.

pub mod corvus;
pub mod event_sink;

use std::sync::Arc;

use arbor_ipc::prelude::{IpcError, LoopbackBroker};
use arbor_shell_common::prelude::{Router, RouterError};
use tauri::AppHandle;

use crate::error::AppError;
use crate::AppState;

/// Build the IPC router with the in-process `corvus` backend registered.
///
/// The loopback dispatch closure owns a clone of `app` so it can reach
/// `AppState` (and fire hooks / emit) while running a handler — exactly what a
/// separate `corvus-be` process would hold as its own state once the backend
/// splits out.
pub fn build_router(app: &AppHandle) -> Router {
    let mut router = Router::new();
    let handle = app.clone();
    router.register(
        "corvus",
        Arc::new(LoopbackBroker::new(move |method, params| {
            corvus::dispatch(&handle, method, params)
        })),
    );
    router
}

/// Forward a product command to its backend through the router and return the
/// JSON result.
///
/// `params` is a JSON object of the handler's named arguments; the result is the
/// handler's return value as JSON (`null` for unit returns). Backend errors
/// arrive as [`IpcError::Backend`] carrying the original `AppError` wire string,
/// which we re-wrap as [`AppError::Other`] so the FE sees the identical message.
pub fn dispatch_rpc(
    state: &AppState,
    program: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let router = state
        .router()
        .ok_or_else(|| AppError::Other("ipc router not initialised".into()))?;
    let bytes = serde_json::to_vec(&params)?;
    let out = router.call(program, method, bytes).map_err(router_err_to_app)?;
    if out.is_empty() {
        Ok(serde_json::Value::Null)
    } else {
        Ok(serde_json::from_slice(&out)?)
    }
}

/// Map a router/transport failure onto the host error enum, preserving the
/// backend wire string the FE matches on.
fn router_err_to_app(e: RouterError) -> AppError {
    match e {
        RouterError::UnknownProduct(p) => AppError::Other(format!("no backend for product '{p}'")),
        RouterError::Ipc(ipc) => match ipc {
            // Already a formatted backend message → pass through verbatim.
            IpcError::Backend(s) => AppError::Other(s),
            IpcError::UnknownMethod(m) => AppError::Other(format!("unknown command: {m}")),
            IpcError::Codec(s) => AppError::Other(format!("ipc codec: {s}")),
            IpcError::Transport(s) => AppError::Other(format!("ipc transport: {s}")),
        },
    }
}
