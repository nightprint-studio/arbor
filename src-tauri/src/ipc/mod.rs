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
pub mod split_broker;

use std::collections::HashSet;
use std::sync::Arc;

use arbor_ipc::prelude::{BrokerClient, ChildClient, IpcError, LoopbackBroker};
use arbor_shell_common::prelude::{Router, RouterError};
use tauri::AppHandle;

use crate::error::AppError;
use crate::ipc::split_broker::SplitBroker;
use crate::AppState;

/// Build the IPC router with the in-process `corvus` backend registered.
///
/// The loopback dispatch closure owns a clone of `app` so it can reach
/// `AppState` (and fire hooks / emit) while running a handler — exactly what a
/// separate `corvus-be` process would hold as its own state once the backend
/// splits out.
pub fn build_router(app: &AppHandle) -> Router {
    let mut router = Router::new();

    // In-process backend: the handlers that still live in the shell run here,
    // against the live `AppState` reached through the captured `AppHandle`.
    let handle = app.clone();
    let loopback: Arc<dyn BrokerClient> = Arc::new(LoopbackBroker::new(move |method, params| {
        corvus::dispatch(&handle, method, params)
    }));

    // Out-of-process backend: try to spawn the real `corvus-be`. Whatever it
    // advertises routes to it; everything else stays in-process. If it can't be
    // spawned (binary not built, spawn error) the shell runs pure in-process —
    // the app must never break on a missing backend.
    match spawn_corvus_be(app) {
        Some((child, methods)) => {
            tracing::info!(
                "corvus-be up: {} method(s) served out-of-process",
                methods.len()
            );
            let oop: HashSet<String> = methods.into_iter().collect();
            router.register(
                "corvus",
                Arc::new(SplitBroker::new(oop, Arc::new(child), loopback)),
            );
        }
        None => {
            tracing::info!("corvus-be not available — all corvus methods in-process");
            router.register("corvus", loopback);
        }
    }

    router
}

/// Locate and spawn the `corvus-be` binary next to the shell executable, wiring
/// its push events back to the FE. Returns the client + its advertised method
/// set, or `None` (logged) if the binary is absent or the spawn fails.
fn spawn_corvus_be(app: &AppHandle) -> Option<(ChildClient, Vec<String>)> {
    use crate::process_ext::NoWindowExt;

    let exe = std::env::current_exe().ok()?;
    let bin = exe
        .parent()?
        .join(format!("corvus-be{}", std::env::consts::EXE_SUFFIX));
    if !bin.exists() {
        tracing::info!(
            "corvus-be binary not found at {} — staying in-process",
            bin.display()
        );
        return None;
    }

    let mut cmd = std::process::Command::new(&bin);
    cmd.no_window(); // no console popup on Windows; stdio piping is unaffected

    let app_for_events = app.clone();
    match ChildClient::spawn(cmd, move |topic, payload| {
        use tauri::Emitter;
        let _ = app_for_events.emit(&topic, payload);
    }) {
        Ok(pair) => Some(pair),
        Err(e) => {
            tracing::warn!("failed to spawn corvus-be ({e}) — staying in-process");
            None
        }
    }
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

/// Push a tab's repo path (plus the currently-resolved git program) to
/// `corvus-be`, so its out-of-process handlers can resolve the tab without the
/// shell's `RepoManager`. Call on repo open.
///
/// **Best-effort**: when `corvus-be` isn't running the `__repo_register` method
/// isn't advertised, so the call routes to the in-process loopback, comes back
/// `UnknownMethod`, and is dropped here — exactly what we want (nothing to sync).
pub fn sync_repo_open(state: &AppState, tab_id: &str, path: &str) {
    let program = crate::git_cli::snapshot()
        .path
        .map(|p| p.to_string_lossy().into_owned());
    let _ = dispatch_rpc(
        state,
        "corvus",
        "__set_git_program",
        serde_json::json!({ "program": program }),
    );
    let _ = dispatch_rpc(
        state,
        "corvus",
        "__repo_register",
        serde_json::json!({ "tab_id": tab_id, "path": path }),
    );
}

/// Forget a tab's repo in `corvus-be`. Best-effort (see [`sync_repo_open`]).
pub fn sync_repo_close(state: &AppState, tab_id: &str) {
    let _ = dispatch_rpc(
        state,
        "corvus",
        "__repo_deregister",
        serde_json::json!({ "tab_id": tab_id }),
    );
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
