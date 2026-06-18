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
//! ## Programs
//!
//! Two backends are registered: **`corvus`** (git — the bulk of the migrated
//! domains, served in-process or by `corvus-be` when present) and
//! **`platform`** (app-agnostic services: config/theme/session/workspace/jobs/
//! fs/terminal/app metadata — in-process only for now, no `platform-be` yet).
//! Each is a router product label; handlers self-register into their program's
//! slice of the `arbor-rpc` inventory (see [`corvus`] / [`platform`]).

pub mod corvus;
pub mod event_sink;
pub mod platform;
pub mod split_broker;
pub mod stream_registry;
pub mod studio;

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

use arbor_ipc::prelude::{BrokerClient, ChildClient, IpcError, LoopbackBroker};
use arbor_rpc::AsyncCallFn;
use arbor_shell_common::prelude::{Router, RouterError};
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::ipc::split_broker::SplitBroker;
use crate::AppState;

/// The **async** handlers, grouped by program, collected once from the
/// `arbor-rpc` inventory. These are network/credential handlers the host awaits
/// directly on the runtime (no blocking-pool thread held for the round-trip) —
/// the bifurcation partner of the sync `registry_for` path that each program's
/// `dispatch` runs on `spawn_blocking`.
fn async_handlers() -> &'static HashMap<&'static str, HashMap<&'static str, AsyncCallFn>> {
    static REG: OnceLock<HashMap<&'static str, HashMap<&'static str, AsyncCallFn>>> = OnceLock::new();
    REG.get_or_init(|| {
        // (wire label, inventory program). `corvus` handlers register under the
        // default/legacy program `""` (a bare `#[corvus::handler]`), so the wire
        // label `"corvus"` maps to the empty inventory program — exactly the
        // remap the sync path does via `corvus::registry()` → `registry_for("")`.
        // `platform`/`studio` register under their own name, so wire == inventory.
        [("corvus", ""), ("platform", "platform"), ("studio", "studio")]
            .into_iter()
            .filter_map(|(wire, inv)| {
                let sub = arbor_rpc::async_registry_for(inv);
                (!sub.is_empty()).then_some((wire, sub))
            })
            .collect()
    })
}

/// Whether `(program, method)` is served by an **async** handler (and so must be
/// awaited on the runtime rather than dispatched on `spawn_blocking`).
pub fn is_async_method(program: &str, method: &str) -> bool {
    async_handlers()
        .get(program)
        .map(|m| m.contains_key(method))
        .unwrap_or(false)
}

/// The methods a running product backend advertises as served **out-of-process**.
/// Populated once in [`build_router`] from `corvus-be`'s `Hello` (today the only
/// OOP backend); unset/empty when no backend is up.
static CORVUS_OOP: OnceLock<HashSet<String>> = OnceLock::new();

/// Whether `(program, method)` is served **out-of-process** by a running product
/// backend. The `rpc` command consults this so it does **not** short-circuit an
/// async method to the in-process [`dispatch_async`] when the backend advertises
/// it — the call must instead flow through the router/`SplitBroker` to the OOP
/// process (otherwise "advertise-and-route" would be a lie for async handlers,
/// pinning every issue-tracker call in-process). When no backend is up the set
/// is unset, so every async method stays in-process — the correct fallback.
///
/// Only `corvus` has an OOP backend today; generalise the key when
/// `platform-be`/`merula-be` arrive.
pub fn is_oop_method(program: &str, method: &str) -> bool {
    program == "corvus" && CORVUS_OOP.get().map(|s| s.contains(method)).unwrap_or(false)
}

/// Await an async handler in-process against the live `AppState`. The future
/// borrows the state for its lifetime; we hold the managed-state guard across
/// the await (the state outlives every request). Errors arrive as the handler's
/// `Display` string, re-wrapped as [`AppError::Other`] (same wire string the FE
/// matches on as the sync path).
pub async fn dispatch_async(
    app: &AppHandle,
    program: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let handler = *async_handlers()
        .get(program)
        .and_then(|m| m.get(method))
        .ok_or_else(|| AppError::Other(format!("no async handler for {program}/{method}")))?;
    let state = app.state::<AppState>();
    let ctx: &(dyn Any + 'static) = &*state;
    handler(ctx, params).await.map_err(AppError::Other)
}

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
            // Publish the OOP set so the `rpc` command stops short-circuiting
            // these methods' async variants to the in-process path (the P0:
            // async methods bypassed the router otherwise — see `is_oop_method`).
            let _ = CORVUS_OOP.set(oop.clone());
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

    // Platform backend: app-agnostic services (config/theme/session/workspace/
    // jobs/fs/terminal/app metadata). In-process only for now — there is no
    // `platform-be` process yet, so it routes straight to the loopback that
    // dispatches against this binary's `platform`-tagged handlers.
    let platform_handle = app.clone();
    let platform_loopback: Arc<dyn BrokerClient> =
        Arc::new(LoopbackBroker::new(move |method, params| {
            platform::dispatch(&platform_handle, method, params)
        }));
    router.register("platform", platform_loopback);

    // Studio backend: the standalone CI/pipeline-config editor. In-process only
    // for now — no `studio-be` process yet — routing to the loopback that
    // dispatches against this binary's `studio`-tagged handlers.
    let studio_handle = app.clone();
    let studio_loopback: Arc<dyn BrokerClient> =
        Arc::new(LoopbackBroker::new(move |method, params| {
            studio::dispatch(&studio_handle, method, params)
        }));
    router.register("studio", studio_loopback);

    router
}

/// Locate and spawn the `corvus-be` binary next to the shell executable, wiring
/// its push events back to the FE. Returns the client + its advertised method
/// set, or `None` (logged) if the binary is absent or the spawn fails.
/// Shell-side host-handler dispatch for the reverse channel
/// (`docs/reverse-channel.md`): answers backend-originated `HostRequest`s. Today
/// that's credential resolution — `__session`/`__refresh` over the
/// descriptor-driven `VaultSessionProvider` (the keyring stays shell-side); the
/// `arbor.ui.*` plugin round-trips join here later. Runs on the `ChildClient`
/// reader thread; `session` is a fast keyring read, a slow `refresh` (OAuth)
/// briefly stalls demux — promote to a worker if that ever bites.
fn host_dispatch(method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    use crate::auth::vault::VaultSessionProvider;
    use arbor_ipc::prelude::SessionProvider;

    let account: String = match method {
        "__session" | "__refresh" => serde_json::from_value(params)
            .map_err(|e| format!("{method}: invalid account: {e}"))?,
        other => return Err(format!("host method not implemented: {other}")),
    };
    let provider = VaultSessionProvider::for_account(&account);
    let is_refresh = method == "__refresh";
    let resolved = tauri::async_runtime::block_on(async move {
        if is_refresh {
            provider.refresh(&account).await
        } else {
            provider.session(&account).await
        }
    });
    match resolved {
        Ok(session) => serde_json::to_value(session).map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Resolve a backend binary (`corvus-be`, …) by its fixed name, env-agnostically:
/// a **dev** build has them sitting **beside** the launcher exe, because cargo
/// co-locates every workspace bin in `target/<profile>/` (so a plain
/// `cargo build -p <name>` lands it there); an **installed** build keeps them in
/// a dedicated `backends/` subfolder, bundled as resources.
///
/// The dev sibling is tried **first** on purpose: it ensures a fresh
/// `cargo build` always wins over a stale release binary that a prior
/// `tauri build` may have left staged in the resource path. Same code path in
/// both environments — no dev/prod flag.
fn backend_binary(app: &AppHandle, name: &str) -> Option<std::path::PathBuf> {
    use tauri::Manager;

    let file = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    // Dev: beside the launcher exe.
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join(&file)))
    {
        if p.exists() {
            return Some(p);
        }
    }
    // Installed: the dedicated `backends/` subfolder under the resource dir.
    let res = app.path().resource_dir().ok()?;
    let p = res.join("backends").join(&file);
    p.exists().then_some(p)
}

fn spawn_corvus_be(app: &AppHandle) -> Option<(ChildClient, Vec<String>)> {
    use crate::process_ext::NoWindowExt;

    let bin = match backend_binary(app, "corvus-be") {
        Some(b) => b,
        None => {
            tracing::info!(
                "corvus-be binary not found (backends/ resource or beside the launcher) — staying in-process"
            );
            return None;
        }
    };

    let mut cmd = std::process::Command::new(&bin);
    cmd.no_window(); // no console popup on Windows; stdio piping is unaffected

    let app_for_events = app.clone();
    match ChildClient::spawn(
        cmd,
        move |topic, payload| {
            use tauri::Emitter;
            let _ = app_for_events.emit(&topic, payload);
        },
        host_dispatch,
    ) {
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
