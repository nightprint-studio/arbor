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
//! (`corvus::stash`), reachable either out-of-process via a `ChildClient`
//! (framed JSON over `corvus-be`'s stdio) or in-process via a [`LoopbackBroker`]
//! whose dispatch closure captures the live `AppHandle`. A `SplitBroker` picks
//! per method: the spawned backend when attached, else the loopback. Hardening
//! the byte-stream to a pipe/socket changes only that client — the router, this
//! module and the `rpc` command don't move.
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
use std::collections::HashMap;
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

/// Whether `(program, method)` is served **out-of-process** by the running
/// `corvus-be`. The `rpc` command consults this so it does **not** short-circuit
/// an async method to the in-process [`dispatch_async`] when the backend
/// advertises it — the call must instead flow through the router/`SplitBroker` to
/// the OOP process (otherwise "advertise-and-route" would be a lie for async
/// handlers, pinning every issue-tracker call in-process). The advertised set is
/// owned by [`split_broker`], attached when `corvus-be` is spawned lazily
/// ([`ensure_corvus_be`]) and detached when it dies — so this reads `false` both
/// before the backend is up and after it disconnects, the correct fallback.
///
/// `corvus` and `merula` each have an OOP backend; `platform`/`studio` are
/// in-process only (no `*-be` yet), so they never advertise OOP methods.
pub fn is_oop_method(program: &str, method: &str) -> bool {
    matches!(program, "corvus" | "merula") && split_broker::serves(program, method)
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

    // Out-of-process backend: `corvus-be` is NOT spawned here. The launcher and
    // the other product windows (explorer, merula) never touch git, so the git
    // backend must not run until the user actually opens Corvus. It is spawned
    // lazily by [`ensure_corvus_be`] when the Corvus window first opens, which
    // splices the child into the shared OOP routing slot (and removes it on
    // disconnect). The broker is registered now but loopback-only; routing flips
    // at attach/detach time without rebuilding the router.
    router.register("corvus", Arc::new(SplitBroker::new("corvus", loopback)));

    // Merula backend: the music live-coding product. Like `corvus`, served
    // out-of-process by `merula-be` — spawned lazily by [`ensure_merula_be`] when
    // the Merula window first opens (the launcher and the other product windows
    // never touch audio). Unlike `corvus`, merula has NO in-process handlers in
    // this shell (the FE invokes the legacy `merula_*` commands directly today),
    // so its loopback is a pure UnknownMethod sink: when `merula-be` is detached
    // every `merula` rpc method falls through to it and the FE shows the down
    // overlay — the intended behaviour. Routing flips at attach/detach time.
    let merula_loopback: Arc<dyn BrokerClient> =
        Arc::new(LoopbackBroker::new(|method, _params| {
            Err(IpcError::UnknownMethod(method.to_string()))
        }));
    router.register("merula", Arc::new(SplitBroker::new("merula", merula_loopback)));

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
fn host_dispatch(
    app: &AppHandle,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::auth::vault::VaultSessionProvider;
    use arbor_ipc::prelude::SessionProvider;
    use tauri::Manager;

    // Job-registry proxy (ADR-3): the shell's `JobRegistry` is the single source
    // of job state + cancellation; an OOP backend drives it over these methods.
    // The backend emits the `arbor://job-*` events itself through its event sink
    // (re-emitted by the shell), so these only mutate the registry.
    if method == "__job_register" {
        let spec: crate::jobs::JobSpec = serde_json::from_value(params)
            .map_err(|e| format!("__job_register: invalid spec: {e}"))?;
        let state = app.state::<AppState>();
        let mut jobs = state
            .jobs
            .lock()
            .map_err(|_| "__job_register: jobs mutex poisoned".to_string())?;
        let id = jobs.new_id();
        jobs.register(spec.into_info(id.clone()));
        return Ok(serde_json::json!(id));
    }
    if method == "__job_append" {
        let job_id = params
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "__job_append: missing job_id".to_string())?;
        let line = params
            .get("line")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if let Ok(mut jobs) = app.state::<AppState>().jobs.lock() {
            jobs.append_output(job_id, line);
        }
        return Ok(serde_json::Value::Null);
    }
    if method == "__job_set_status" {
        let job_id = params
            .get("job_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "__job_set_status: missing job_id".to_string())?
            .to_string();
        let status: crate::jobs::JobStatus =
            serde_json::from_value(params.get("status").cloned().unwrap_or(serde_json::Value::Null))
                .map_err(|e| format!("__job_set_status: invalid status: {e}"))?;
        if let Ok(mut jobs) = app.state::<AppState>().jobs.lock() {
            jobs.set_status(&job_id, status);
        }
        return Ok(serde_json::Value::Null);
    }
    if method == "__job_is_cancelled" {
        let job_id: String = serde_json::from_value(params)
            .map_err(|e| format!("__job_is_cancelled: invalid job_id: {e}"))?;
        let cancelled = app
            .state::<AppState>()
            .jobs
            .lock()
            .map(|j| j.is_cancelled(&job_id))
            .unwrap_or(false);
        return Ok(serde_json::json!(cancelled));
    }

    // `arbor.job.*` PROXY ops not covered by the `__job_register`/`__job_append`/
    // `__job_set_status`/`__job_is_cancelled` family above. `corvus-be`'s
    // `arbor.job.*` namespace reserves the id via `__job_register` (above), then
    // drives the real spawn + registry reads/mutations here. These mirror
    // `ns_shell/job.rs` byte-for-byte. `__job_spawn` emits `arbor://job-started`
    // and runs `crate::jobs::spawn_job` (the job is already registered).
    if method == "__job_spawn" {
        use tauri::Emitter;
        let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("Job").to_string();
        let plugin_name =
            params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let command = params.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let cwd = params.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
        let category = params.get("category").and_then(|v| v.as_str()).map(|s| s.to_string());
        let hidden = params.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false);
        let target = params.get("target").and_then(|v| v.as_str()).map(|s| s.to_string());
        let on_done_action =
            params.get("on_done_action").and_then(|v| v.as_str()).map(|s| s.to_string());
        let env: Vec<(String, String)> = params
            .get("env")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let _ = app.emit(
            "arbor://job-started",
            serde_json::json!({
                "job_id":      &job_id,
                "name":        &name,
                "plugin_name": &plugin_name,
                "command":     &command,
                "category":    &category,
                "hidden":      hidden,
                "target":      &target,
            }),
        );

        crate::jobs::spawn_job(
            crate::jobs::JobSpawnRequest {
                job_id,
                name,
                plugin_name,
                command,
                cwd,
                env,
                on_done_action,
                category,
            },
            app.clone(),
        );
        return Ok(serde_json::Value::Null);
    }
    if method == "__job_list" {
        let list = match app.state::<AppState>().jobs.lock() {
            Ok(g) => g.list(),
            Err(e) => return Err(format!("job.list lock: {e}")),
        };
        return serde_json::to_value(&list).map_err(|e| format!("job.list encode: {e}"));
    }
    if method == "__job_cancel" {
        let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or_default();
        if let Ok(mut jobs) = app.state::<AppState>().jobs.lock() {
            jobs.cancel(job_id);
        }
        return Ok(serde_json::Value::Null);
    }
    if method == "__job_dismiss" {
        let job_id = params.get("job_id").and_then(|v| v.as_str()).unwrap_or_default();
        let dismissed = if let Ok(mut jobs) = app.state::<AppState>().jobs.lock() {
            jobs.dismiss(job_id)
        } else {
            false
        };
        return Ok(serde_json::json!(dismissed));
    }
    if method == "__job_clear_finished" {
        let cleared: Vec<String> = if let Ok(mut jobs) = app.state::<AppState>().jobs.lock() {
            jobs.clear_finished()
        } else {
            Vec::new()
        };
        return serde_json::to_value(&cleared).map_err(|e| format!("job.clear_finished encode: {e}"));
    }

    // Toolchain-registry proxy: the `ToolchainRegistry` lives in the shell's
    // `AppState` (`toolchain_registry`), so `corvus-be`'s `arbor.toolchain.*`
    // namespace (a PROXY installer) round-trips each op here. These mirror
    // `ns_shell/toolchain.rs` byte-for-byte — same registry calls, same
    // `toolchain.<op>[ lock| encode]: …` error strings, same return shapes (the
    // installer surfaces this `String` verbatim to Lua as `(nil|false, err)`).
    if method == "__toolchain_list" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let entries = match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => g.list(kind),
            Err(e) => return Err(format!("toolchain.list lock: {e}")),
        };
        return serde_json::to_value(&entries).map_err(|e| format!("toolchain.list encode: {e}"));
    }
    if method == "__toolchain_active" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let entry = match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => g.active(kind),
            Err(e) => return Err(format!("toolchain.active lock: {e}")),
        };
        return match entry {
            None => Ok(serde_json::Value::Null),
            Some(e) => serde_json::to_value(&e).map_err(|e| format!("toolchain.active encode: {e}")),
        };
    }
    if method == "__toolchain_env" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let id = params.get("id").and_then(|v| v.as_str());
        let env = match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => g.env_for(kind, id),
            Err(e) => return Err(format!("toolchain.env lock: {e}")),
        };
        return serde_json::to_value(&env).map_err(|e| format!("toolchain.env encode: {e}"));
    }
    if method == "__toolchain_detect" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let entries = match app.state::<AppState>().toolchain_registry.lock() {
            Ok(g) => g.detect(kind),
            Err(e) => return Err(format!("toolchain.detect lock: {e}")),
        };
        return serde_json::to_value(&entries).map_err(|e| format!("toolchain.detect encode: {e}"));
    }
    if method == "__toolchain_add" {
        let kind = params
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        // The entry shape was already validated installer-side; deserialize it back
        // into the typed `ToolchainEntry` the registry stores.
        let entry: arbor_plugin_core::prelude::ToolchainEntry =
            serde_json::from_value(params.get("entry").cloned().unwrap_or(serde_json::Value::Null))
                .map_err(|e| format!("toolchain.add: invalid entry: {e}"))?;
        return match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => {
                g.add(&kind, entry);
                Ok(serde_json::Value::Null)
            }
            Err(e) => Err(format!("toolchain.add lock: {e}")),
        };
    }
    if method == "__toolchain_remove" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        return match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => {
                g.remove(kind, id);
                Ok(serde_json::Value::Null)
            }
            Err(e) => Err(format!("toolchain.remove lock: {e}")),
        };
    }
    if method == "__toolchain_set_active" {
        let kind = params.get("kind").and_then(|v| v.as_str()).unwrap_or_default();
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        return match app.state::<AppState>().toolchain_registry.lock() {
            Ok(mut g) => {
                g.set_active(kind, id);
                Ok(serde_json::Value::Null)
            }
            Err(e) => Err(format!("toolchain.set_active lock: {e}")),
        };
    }

    // `arbor.ui.set_branding` (PROXY). Mirrors `ns_shell/ui/branding.rs`: apply the
    // OS window-icon BEFORE writing AppState.branding (so a Tauri error leaves the
    // previous override intact), then emit `arbor://branding-changed`. The pure
    // validation (svg/svg_path/icon-path checks, svg_path read) already ran
    // installer-side; `svg` here is the resolved inline body. The error string
    // matches the shell's nested `window_icon_path failed: {read icon|set_icon}: …`.
    if method == "__set_branding" {
        let svg = params.get("svg").and_then(|v| v.as_str()).map(|s| s.to_string());
        let icon_path =
            params.get("window_icon_path").and_then(|v| v.as_str()).map(|s| s.to_string());
        let pname = params.get("plugin").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        // Apply the OS-level icon BEFORE writing the state — a Tauri error then
        // still leaves the previous override intact.
        if let Some(ref p) = icon_path {
            let img = tauri::image::Image::from_path(p)
                .map_err(|e| format!("arbor.ui.set_branding: window_icon_path failed: read icon: {e}"))?;
            let win = app
                .get_webview_window("main")
                .ok_or_else(|| "arbor.ui.set_branding: window_icon_path failed: no 'main' window".to_string())?;
            win.set_icon(img)
                .map_err(|e| format!("arbor.ui.set_branding: window_icon_path failed: set_icon: {e}"))?;
        }
        let state = app.state::<AppState>();
        state.branding.apply(svg, icon_path, pname);
        crate::commands::branding_commands::emit_branding_changed(app, &state.branding.snapshot());
        return Ok(serde_json::Value::Null);
    }

    // `arbor.ui.clear_branding` (PROXY). Only clears when this plugin owns the
    // override; restores the bundled window icon when the cleared state carried one;
    // emits `arbor://branding-changed`.
    if method == "__clear_branding" {
        let pname = params.get("plugin").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let state = app.state::<AppState>();
        // Only clear if THIS plugin owns the override — protects against a noisy
        // plugin nuking another plugin's branding when it unloads.
        let Some(prev) = state.branding.clear(Some(&pname)) else {
            return Ok(serde_json::Value::Null);
        };
        // If the previous state included a window icon, restore the bundled default
        // so the taskbar doesn't keep showing stale art.
        if prev.window_icon_path.is_some() {
            if let Some(win) = app.get_webview_window("main") {
                if let Some(default) = app.default_window_icon() {
                    let _ = win.set_icon(default.clone());
                }
            }
        }
        crate::commands::branding_commands::emit_branding_changed(app, &state.branding.snapshot());
        return Ok(serde_json::Value::Null);
    }

    // `arbor.ui.set_theme_tokens` (PROXY). Frontend-only: rebroadcast the overlay
    // via `arbor://theme-overlay`. The `{ "--x": v, … }` vars object was built +
    // validated installer-side.
    if method == "__set_theme_overlay" {
        let pname = params.get("plugin").and_then(|v| v.as_str()).unwrap_or_default();
        let vars = params
            .get("vars")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        crate::commands::branding_commands::emit_theme_overlay(app, pname, &vars);
        return Ok(serde_json::Value::Null);
    }

    // `arbor.ui.clear_theme_tokens` (PROXY). Empty-vars payload is the agreed
    // "release my overlay" signal — the frontend deletes the entry keyed by plugin.
    if method == "__clear_theme_overlay" {
        let pname = params.get("plugin").and_then(|v| v.as_str()).unwrap_or_default();
        crate::commands::branding_commands::emit_theme_overlay(
            app,
            pname,
            &serde_json::Value::Object(serde_json::Map::new()),
        );
        return Ok(serde_json::Value::Null);
    }

    // Current plugin-applied logo override (branding is in-memory shell state):
    // an OOP report/export embeds it. `None` when no plugin set one.
    if method == "__branding_logo" {
        let logo = app.state::<AppState>().branding.snapshot().logo_svg;
        return Ok(serde_json::json!(logo));
    }

    // Keyring-coupled `has_token` probe for the OOP `get_ci_provider`: the pure
    // URL detection runs in `corvus-be`, but whether a credential is stored for
    // the detected provider/host is a keyring read that stays shell-side.
    if method == "__has_token" {
        let provider = params.get("provider").and_then(|v| v.as_str()).unwrap_or_default();
        let base = params.get("gitlab_base_url").and_then(|v| v.as_str());
        let has = crate::git_provider::ci_impl::has_token_for(provider, base);
        return Ok(serde_json::json!(has));
    }

    // Proactive provider-keyed refresh — the OOP twin of the in-process
    // `maybe_refresh_for_provider` pre-call an OOP REST handler can no longer
    // make directly (the keyring is shell-side). Param is the provider string
    // (`"github"` | `"gitlab"`); failures are swallowed exactly as in-process.
    if method == "__maybe_refresh" {
        let provider: String = serde_json::from_value(params)
            .map_err(|e| format!("__maybe_refresh: invalid provider: {e}"))?;
        tauri::async_runtime::block_on(crate::auth::maybe_refresh_for_provider(&provider));
        return Ok(serde_json::Value::Null);
    }

    // Git smart-HTTP credentials for an OOP `remote` op: the keyring stays
    // shell-side, so an OOP fetch/push marshals `(url) -> Option<(user, pass)>`
    // here. This is the HTTP-Basic pair (`credential_store::resolve_credentials`),
    // distinct from the REST `AuthSession` of `__session`.
    if method == "__git_credentials" {
        let url: String = serde_json::from_value(params)
            .map_err(|e| format!("__git_credentials: invalid url: {e}"))?;
        let creds = crate::auth::credential_store::resolve_credentials(&url)
            .map_err(|e| e.to_string())?;
        return serde_json::to_value(creds).map_err(|e| e.to_string());
    }

    // Proactive URL-keyed token refresh — the OOP twin of the in-process
    // `maybe_refresh_for_url` pre-call a fetch/push makes before resolving creds.
    if method == "__maybe_refresh_url" {
        let url: String = serde_json::from_value(params)
            .map_err(|e| format!("__maybe_refresh_url: invalid url: {e}"))?;
        tauri::async_runtime::block_on(crate::auth::maybe_refresh_for_url(&url));
        return Ok(serde_json::Value::Null);
    }

    // corvus-be self-detects git but cannot write the profile-aware `config.toml`
    // (the shell owns the active profile). On a Settings change it asks the shell
    // to persist the `[git] executable_path` override (or clear it when null) and
    // re-detect the shell's OWN in-process git so its direct shell-outs match.
    if method == "__persist_git_path" {
        let path = params.get("path").and_then(|v| v.as_str()).map(|s| s.to_string());
        let st = app.state::<AppState>();
        {
            let mut cfg = st.lock_config().map_err(|e| e.to_string())?;
            cfg.git.executable_path = path.clone();
            crate::config::app_config::save(&cfg).map_err(|e| e.to_string())?;
        }
        crate::git_cli::detect(path.as_deref().map(std::path::Path::new));
        return Ok(serde_json::Value::Null);
    }

    // Registry-orphan GC (ADR-1): corvus-be owns the repo registry + workspace
    // store, but `recent_repos` is a shell `AppConfig` slice. When corvus-be
    // forgets an orphaned repo it asks the shell to drop the matching recent-repos
    // pointer too, so a later import no longer offers it as "use existing".
    if method == "__forget_recent_repo" {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default();
        let st = app.state::<AppState>();
        let _ = crate::commands::workspace_commands::forget_recent_repo(&st, path);
        return Ok(serde_json::Value::Null);
    }

    // corvus-be's missing-repo flow READS the shell's recent_repos list (a
    // GENERIC_KEYS / profile.toml slice the launcher recents share — NOT corvus's
    // to own) to find dead entries to prune.
    if method == "__recent_repos_list" {
        let st = app.state::<AppState>();
        let list = st.lock_config().map(|c| c.recent_repos.clone()).unwrap_or_default();
        return serde_json::to_value(list).map_err(|e| e.to_string());
    }

    // …and PREPENDS a path (used by relocate_repo's mirror). Replicates the
    // platform add_recent_repo handler inline: normalise to forward slashes,
    // dedup, prepend, cap at 10 (MAX_RECENT).
    if method == "__recent_repos_add" {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default();
        if !path.trim().is_empty() {
            let st = app.state::<AppState>();
            if let Ok(mut cfg) = st.lock_config() {
                let normalized = path.replace('\\', "/");
                cfg.recent_repos.retain(|p| p.replace('\\', "/") != normalized);
                cfg.recent_repos.insert(0, normalized);
                cfg.recent_repos.truncate(10);
                let _ = crate::config::app_config::save(&cfg);
            };
        }
        return Ok(serde_json::Value::Null);
    }

    // Populate the shell's RepoManager for a tab corvus-be just init'd OOP, so the
    // shell-side in-process consumers (studio, git_provider helpers, plugin
    // ns_shell, remote_commands) resolve it. Mirrors `open_repo`'s `mgr.open`.
    // corvus-be has already self-registered the tab in its own registry, so we do
    // NOT re-call `sync_repo_open` here (that would round-trip `__repo_register`).
    if method == "__shell_open_repo" {
        let tab_id = params.get("tab_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let path   = params.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let state = app.state::<AppState>();
        {
            let mut mgr = state.lock_repos().map_err(|e| e.to_string())?;
            mgr.open(tab_id, &path).map_err(|e| e.to_string())?;
        };
        return Ok(serde_json::Value::Null);
    }

    // GitLab namespace path -> numeric namespace_id (keyring read + REST stay
    // shell-side). Lifted from the deleted shell `resolve_gitlab_namespace_id`.
    // Best-effort: any failure / no-match -> JSON null, which corvus-be reads as
    // `None` (falls through to the user's default namespace).
    if method == "__gitlab_namespace_id" {
        let path = params.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let id: Option<u64> = tauri::async_runtime::block_on(async move {
            let token = crate::auth::credential_store::get("gitlab.com/arbor", "oauth")
                .ok()
                .flatten()
                .or_else(|| {
                    crate::auth::credential_store::get_for_host("gitlab.com")
                        .ok()
                        .flatten()
                        .map(|(_, tok)| tok)
                });
            let token = token?;
            let url = format!("https://gitlab.com/api/v4/namespaces?search={path}");
            let resp = reqwest::Client::new()
                .get(&url)
                .header("Authorization", format!("Bearer {token}"))
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() { return None; }
            let arr = resp.json::<serde_json::Value>().await.ok()?;
            arr.as_array()
                .and_then(|a| a.iter().find(|n| n["path"].as_str() == Some(path.as_str())).or_else(|| a.first()))
                .and_then(|n| n["id"].as_u64())
        });
        return Ok(serde_json::json!(id));
    }

    // ── arbor.pipeline.* PROXY ops ──────────────────────────────────────────
    //
    // The `PipelineEngine` / `PipelineRuntime` live in the shell's `AppState`, so
    // `corvus-be`'s `arbor.pipeline.*` namespace (a PROXY installer) round-trips
    // each host-touching op here. These mirror `ns_shell/pipeline.rs` — same
    // registry calls, same `pipeline.<op>: …` error strings, same return shapes.
    if method == "__pipeline_define" {
        use tauri::Emitter;
        let plugin_name = params
            .get("plugin_name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let mut config = params.get("config").cloned().unwrap_or(serde_json::Value::Null);
        if let Some(obj) = config.as_object_mut() {
            obj.insert(
                "plugin".to_string(),
                serde_json::Value::String(plugin_name.clone()),
            );
        }
        let def: crate::pipeline::PipelineDef = serde_json::from_value(config)
            .map_err(|e| format!("arbor.pipeline.define: invalid config: {e}"))?;
        let def_id = def.id.clone();
        let state = app.state::<AppState>();
        state
            .pipeline_engine
            .registry
            .lock()
            .map_err(|e| format!("pipeline.define lock: {e}"))?
            .register_def(def);
        let _ = app.emit(
            "arbor://pipeline-def-registered",
            serde_json::json!({ "pipeline_id": def_id, "plugin": plugin_name }),
        );
        return Ok(serde_json::Value::Null);
    }
    if method == "__pipeline_run" {
        let plugin_name = params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or_default();
        let pipeline_id = params.get("pipeline_id").and_then(|v| v.as_str()).unwrap_or_default();
        let override_cwd = params.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
        let silent_override = params.get("silent").and_then(|v| v.as_bool());
        let state = app.state::<AppState>();

        let def = {
            let reg = state
                .pipeline_engine
                .registry
                .lock()
                .map_err(|e| format!("pipeline.run lock: {e}"))?;
            match reg.defs.iter().find(|d| d.id == pipeline_id && d.plugin == plugin_name).cloned() {
                Some(d) => d,
                None => return Err(format!("pipeline.run: pipeline '{pipeline_id}' not found")),
            }
        };

        let repo_path = override_cwd.or_else(|| {
            state
                .active_tab_id
                .lock()
                .ok()
                .and_then(|tid| tid.clone())
                .and_then(|tid| {
                    state
                        .repos
                        .lock()
                        .ok()
                        .and_then(|mut mgr| mgr.get(&tid).ok().map(|r| r.path.clone()))
                })
        });

        let run_id = match state.pipeline_engine.registry.lock() {
            Ok(mut reg) => reg.new_run_id(),
            Err(e) => return Err(format!("pipeline.run lock: {e}")),
        };
        let mut run = def.new_run(run_id.clone(), repo_path.clone());
        if let Some(s) = silent_override {
            run.silent = s;
        }
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        match state.pipeline_engine.registry.lock() {
            Ok(mut reg) => reg.add_run(run, cancel.clone()),
            Err(e) => return Err(format!("pipeline.run add_run: {e}")),
        }
        let rt = match state.pipeline_runtime() {
            Some(rt) => std::sync::Arc::new(rt),
            None => return Err("pipeline.run: runtime unavailable".to_string()),
        };
        crate::pipeline::start_pipeline_run(def, run_id.clone(), repo_path, cancel, rt);
        return Ok(serde_json::json!(run_id));
    }
    if method == "__pipeline_resume" {
        let run_id = params.get("run_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let state = app.state::<AppState>();
        let Some(rt) = state.pipeline_runtime() else {
            return Err("pipeline.resume: runtime unavailable".to_string());
        };
        return crate::pipeline::resume_run(&run_id, std::sync::Arc::new(rt))
            .map(|_| serde_json::Value::Null)
            .map_err(|e| format!("pipeline.resume: {e}"));
    }
    if method == "__pipeline_discard" {
        let run_id = params.get("run_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let state = app.state::<AppState>();
        let Some(rt) = state.pipeline_runtime() else {
            return Err("pipeline.discard: runtime unavailable".to_string());
        };
        return crate::pipeline::discard_run(&run_id, std::sync::Arc::new(rt))
            .map(|_| serde_json::Value::Null)
            .map_err(|e| format!("pipeline.discard: {e}"));
    }
    if method == "__pipeline_is_locked" {
        let lock_key = params.get("lock_key").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        let reg = match state.pipeline_engine.registry.lock() {
            Ok(g) => g,
            Err(e) => return Err(format!("pipeline.is_locked lock: {e}")),
        };
        return Ok(match reg.locked_by(lock_key) {
            Some(id) => serde_json::Value::String(id.to_string()),
            None => serde_json::Value::Null,
        });
    }
    if method == "__pipeline_list" {
        let plugin_name = params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        let reg = match state.pipeline_engine.registry.lock() {
            Ok(g) => g,
            Err(e) => return Err(format!("pipeline.list lock: {e}")),
        };
        let defs: Vec<_> = reg.defs.iter().filter(|d| d.plugin == plugin_name).collect();
        return serde_json::to_value(&defs).map_err(|e| format!("pipeline.list encode: {e}"));
    }
    if method == "__pipeline_get" {
        let plugin_name = params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or_default();
        let id = params.get("id").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        let reg = match state.pipeline_engine.registry.lock() {
            Ok(g) => g,
            Err(e) => return Err(format!("pipeline.get lock: {e}")),
        };
        return match reg.defs.iter().find(|d| d.id == id && d.plugin == plugin_name) {
            Some(def) => serde_json::to_value(def).map_err(|e| format!("pipeline.get encode: {e}")),
            None => Ok(serde_json::Value::Null),
        };
    }
    if method == "__pipeline_cancel" {
        let run_id = params.get("run_id").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        if let Ok(mut reg) = state.pipeline_engine.registry.lock() {
            reg.cancel(run_id);
        }
        state.pipeline_engine.cv.notify_all();
        return Ok(serde_json::Value::Null);
    }
    if method == "__pipeline_list_runs" {
        let default_plugin = params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or_default();
        let filter_plugin = params.get("plugin").and_then(|v| v.as_str());
        let filter_pipeline_id = params.get("pipeline_id").and_then(|v| v.as_str());
        let all = params.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
        let state = app.state::<AppState>();
        let reg = match state.pipeline_engine.registry.lock() {
            Ok(g) => g,
            Err(e) => return Err(format!("pipeline.list_runs lock: {e}")),
        };
        let plugin_scope: Option<String> = if all {
            None
        } else {
            Some(filter_plugin.unwrap_or(default_plugin).to_string())
        };
        let runs: Vec<_> = reg
            .runs
            .iter()
            .filter(|r| plugin_scope.as_deref().map_or(true, |p| r.plugin == p))
            .filter(|r| filter_pipeline_id.map_or(true, |id| r.pipeline_id == id))
            .cloned()
            .collect();
        return serde_json::to_value(&runs).map_err(|e| format!("pipeline.list_runs encode: {e}"));
    }
    if method == "__pipeline_get_run" {
        let run_id = params.get("run_id").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        let reg = match state.pipeline_engine.registry.lock() {
            Ok(g) => g,
            Err(e) => return Err(format!("pipeline.get_run lock: {e}")),
        };
        return match reg.get_run(run_id) {
            Some(r) => serde_json::to_value(r).map_err(|e| format!("pipeline.get_run encode: {e}")),
            None => Ok(serde_json::Value::Null),
        };
    }
    if method == "__pipeline_list_ops" {
        let ops = match app.state::<AppState>().plugin_host.lock() {
            Ok(host) => host.list_all_pipeline_ops(),
            Err(_) => Vec::new(),
        };
        return serde_json::to_value(&ops).map_err(|e| format!("pipeline.list_ops encode: {e}"));
    }

    // `arbor.brp.*` PROXY: the `BrpRegistry` (live HTTP client + SSE
    // subscriptions) lives in the shell's `AppState.brp`, so `corvus-be`'s
    // `arbor.brp.*` namespace round-trips each op here. These mirror
    // `ns_shell/brp.rs`. `host_dispatch` is sync, so the async probe / call are
    // driven with `tauri::async_runtime::block_on`. `__brp_watch` runs the SSE
    // stream here in the shell, but each event is pushed back to `corvus-be`'s
    // parked watch callback over the inverse `invoke_plugin_callback` RPC (keyed
    // by the `{ plugin, callback_id }` the BE forwards in `__watch_meta`); the
    // subscription is parked on the `BrpRegistry` so `__brp_unwatch` (and the
    // stream-end teardown) can drop both the stream and the parked closure.
    if method == "__brp_connect" {
        use corvus_brp::prelude::{
            probe_capabilities, BrpClient, BrpSession, BrpStatus, DEFAULT_ENDPOINT,
        };
        use std::time::Duration;
        let endpoint = params
            .get("endpoint")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(DEFAULT_ENDPOINT)
            .to_string();
        let timeout_ms = params.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(5_000);
        let client = match BrpClient::new(endpoint.clone(), Duration::from_millis(timeout_ms)) {
            Ok(c) => c,
            Err(e) => return Ok(brp_error_envelope_value(e)),
        };
        let caps = match tauri::async_runtime::block_on(probe_capabilities(&client)) {
            Ok(c) => c,
            Err(e) => return Ok(brp_error_envelope_value(e)),
        };
        let session = BrpSession::new(endpoint, client).with_capabilities(caps);
        let status = BrpStatus::from_session(Some(&session));
        match app.state::<AppState>().brp.lock() {
            Ok(mut reg) => reg.set(session),
            Err(_) => return Ok(serde_json::json!({ "ok": false, "error": { "kind": "internal", "message": "brp registry mutex poisoned" } })),
        }
        return serde_json::to_value(&status)
            .map(|s| serde_json::json!({ "ok": true, "result": s }))
            .map_err(|e| format!("brp.connect encode: {e}"));
    }
    if method == "__brp_disconnect" {
        use corvus_brp::prelude::BrpStatus;
        if let Ok(mut reg) = app.state::<AppState>().brp.lock() {
            reg.clear();
        }
        return serde_json::to_value(BrpStatus::from_session(None))
            .map_err(|e| format!("brp.disconnect encode: {e}"));
    }
    if method == "__brp_status" {
        use corvus_brp::prelude::BrpStatus;
        let status = app
            .state::<AppState>()
            .brp
            .lock()
            .map(|reg| BrpStatus::from_session(reg.session()))
            .unwrap_or_else(|_| BrpStatus::from_session(None));
        return serde_json::to_value(&status).map_err(|e| format!("brp.status encode: {e}"));
    }
    if method == "__brp_call" {
        let method_name = params.get("method").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let call_params = params.get("params").cloned().filter(|v| !v.is_null());
        let client = match app.state::<AppState>().brp.lock() {
            Ok(reg) => reg.session().map(|s| s.client.clone()),
            Err(_) => return Ok(serde_json::json!({ "ok": false, "error": { "kind": "internal", "message": "brp registry mutex poisoned" } })),
        };
        let Some(client) = client else {
            return Ok(serde_json::json!({ "ok": false, "error": { "kind": "not_connected", "message": "BRP not connected — call arbor.brp.connect first" } }));
        };
        return Ok(match tauri::async_runtime::block_on(client.call(&method_name, call_params)) {
            Ok(value) => serde_json::json!({ "ok": true, "result": value }),
            Err(e) => brp_error_envelope_value(e),
        });
    }
    if method == "__brp_watch" {
        use corvus_brp::prelude::{run_watch_stream, WatchSub};
        let method_name = params.get("method").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        // corvus-be wraps the real BRP params alongside the routing metadata it
        // needs to deliver SSE events back to its parked Lua closure:
        // `params = { __watch_meta: { plugin, callback_id }, params: <real> }`.
        // Pop the metadata; the inner `params` is the actual BRP request body.
        let meta = params.get("__watch_meta");
        let plugin = meta
            .and_then(|m| m.get("plugin"))
            .and_then(|v| v.as_str())
            .unwrap_or("corvus-be")
            .to_string();
        let callback_id = meta
            .and_then(|m| m.get("callback_id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let watch_params = params.get("params").cloned().filter(|v| !v.is_null());
        let state = app.state::<AppState>();
        let (endpoint, sub_id) = match state.brp.lock() {
            Ok(mut reg) => match reg.session() {
                Some(s) => {
                    let ep = s.endpoint.clone();
                    (ep, reg.next_watch_id())
                }
                None => return Err("BRP not connected — call arbor.brp.connect first".to_string()),
            },
            Err(e) => return Err(format!("brp.watch lock: {e}")),
        };
        // Each SSE event is shaped into the Lua watch envelope and pushed back to
        // corvus-be's parked callback over `invoke_plugin_callback`. On stream-end
        // the parked closure is dropped via `remove_plugin_callback`. On abort
        // (the `__brp_unwatch` path) the task drops mid-poll before stream-end, so
        // teardown there drives `remove_plugin_callback` itself.
        let app_for_task = app.clone();
        let plugin_for_task = plugin.clone();
        let callback_for_task = callback_id.clone();
        let join = tokio::spawn(async move {
            let app_ev = app_for_task.clone();
            let plugin_ev = plugin_for_task.clone();
            let callback_ev = callback_for_task.clone();
            run_watch_stream(endpoint, method_name, watch_params, move |event| {
                let payload = watch_event_to_payload(&event);
                let payload_json = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
                invoke_plugin_callback_on_backend(&app_ev, &plugin_ev, &callback_ev, &payload_json);
            })
            .await;
            // Stream closed on its own (not aborted) → free the parked closure.
            let state = app_for_task.state::<AppState>();
            remove_plugin_callback_on_backend(&state, &plugin_for_task, &callback_for_task);
        });
        if let Ok(mut reg) = state.brp.lock() {
            reg.insert_watch(WatchSub {
                id: sub_id,
                plugin,
                method: params.get("method").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                // Store the BE callback id so `__brp_unwatch` can drop the parked
                // closure in the backend VM when the user tears the stream down.
                hook_name: callback_id,
                aborter: join.abort_handle(),
            });
        }
        return Ok(serde_json::json!(sub_id));
    }
    if method == "__brp_unwatch" {
        let sub_id = params.get("sub_id").and_then(|v| v.as_u64()).unwrap_or(0);
        if sub_id == 0 {
            return Ok(serde_json::json!(false));
        }
        let state = app.state::<AppState>();
        // Take the sub out and release the brp lock BEFORE the teardown RPC — the
        // `remove_plugin_callback` round-trip must not run under `state.brp`.
        let taken = match state.brp.lock() {
            Ok(mut reg) => reg.take_watch(sub_id),
            Err(_) => None,
        };
        let removed = match taken {
            Some(sub) => {
                sub.aborter.abort();
                // Abort drops the stream task mid-poll, so its stream-end teardown
                // never runs — drop the parked closure in the backend VM here.
                if !sub.hook_name.is_empty() {
                    remove_plugin_callback_on_backend(&state, &sub.plugin, &sub.hook_name);
                }
                true
            }
            None => false,
        };
        return Ok(serde_json::json!(removed));
    }

    // ── cloud (`arbor.cloud.*`) PROXY handlers ──────────────────────────────────
    // corvus-be's `arbor.cloud.*` namespace round-trips here. Each block re-runs
    // the body of the matching `ns_shell/cloud.rs` installer closure, reading args
    // from the JSON `params` (= the serde of the Lua opts table) and using
    // `tauri::async_runtime::block_on` in place of the `block_on!` macro. All field
    // validation + error strings are byte-identical to `ns_shell/cloud.rs`.
    // `arbor-cloud` paths reach the shared logic via `crate::cloud::{ops,transfer,
    // oauth_google,secrets,types}`.
    if method == "__cloud_secret_set" {
        let r = params.get("secret_ref").and_then(|v| v.as_str()).unwrap_or_default();
        let v = params.get("value").and_then(|v| v.as_str()).unwrap_or_default();
        return crate::cloud::secrets::set(r, v).map(|_| serde_json::Value::Null).map_err(|e| e.to_string());
    }
    if method == "__cloud_secret_exists" {
        let r = params.get("secret_ref").and_then(|v| v.as_str()).unwrap_or_default();
        return crate::cloud::secrets::exists(r).map(serde_json::Value::Bool).map_err(|e| e.to_string());
    }
    if method == "__cloud_secret_delete" {
        let r = params.get("secret_ref").and_then(|v| v.as_str()).unwrap_or_default();
        return crate::cloud::secrets::delete(r).map(|_| serde_json::Value::Null).map_err(|e| e.to_string());
    }
    if method == "__cloud_test_connection" {
        let op = "arbor.cloud.test_connection";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str());
        let r = tauri::async_runtime::block_on(crate::cloud::ops::test_connection(&conn, bucket))
            .map_err(|e| e.to_string())?;
        return serde_json::to_value(&r).map_err(|e| format!("{op} encode: {e}"));
    }
    if method == "__cloud_test_connection_async" {
        let op = "arbor.cloud.test_connection_async";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).map(|s| s.to_string());
        let on_done = params.get("on_done").and_then(|v| v.as_str())
            .ok_or_else(|| format!("{op}: missing required field `on_done`"))?.to_string();
        let request_id = params.get("request_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let app2 = app.clone();
        tauri::async_runtime::spawn(async move {
            let res = crate::cloud::ops::test_connection(&conn, bucket.as_deref()).await;
            let payload = match res {
                Ok(r)  => serde_json::json!({ "request_id": request_id, "ok": true,  "reply": r }),
                Err(e) => serde_json::json!({ "request_id": request_id, "ok": false, "error": e.to_string() }),
            };
            let payload_str = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string());
            std::thread::spawn(move || {
                let state = app2.state::<AppState>();
                if let Ok(host) = state.plugin_host.lock() {
                    arbor_plugin_core::prelude::fire_broadcast(&host, &on_done, &payload_str);
                };
            });
        });
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_list" {
        let op = "arbor.cloud.list";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str())
            .ok_or_else(|| format!("{op}: missing required field `bucket`"))?;
        let prefix = params.get("prefix").and_then(|v| v.as_str()).unwrap_or_default();
        let limit = params.get("limit").and_then(|v| v.as_i64()).map(|n| n.max(0) as usize);
        let page = tauri::async_runtime::block_on(crate::cloud::ops::list(&conn, bucket, prefix, limit))
            .map_err(|e| e.to_string())?;
        return serde_json::to_value(&page).map_err(|e| format!("{op} encode: {e}"));
    }
    if method == "__cloud_stat" {
        let op = "arbor.cloud.stat";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?;
        let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `path`"))?;
        let o = tauri::async_runtime::block_on(crate::cloud::ops::stat(&conn, bucket, path)).map_err(|e| e.to_string())?;
        return serde_json::to_value(&o).map_err(|e| format!("{op} encode: {e}"));
    }
    if method == "__cloud_delete" {
        let op = "arbor.cloud.delete";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?;
        let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `path`"))?;
        let recursive = params.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);
        tauri::async_runtime::block_on(crate::cloud::ops::delete(&conn, bucket, path, recursive)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_copy" {
        let op = "arbor.cloud.copy";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?;
        let src = params.get("src").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `src`"))?;
        let dst = params.get("dst").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `dst`"))?;
        tauri::async_runtime::block_on(crate::cloud::ops::copy(&conn, bucket, src, dst)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_list_stream" {
        let op = "arbor.cloud.list_stream";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `stream_id`"))?.to_string();
        let prefix = params.get("prefix").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let cap = params.get("cap").and_then(|v| v.as_i64()).map(|n| n.max(0) as usize);
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let state = app.state::<AppState>();
            if let Ok(mut map) = state.cloud_cancellations.lock() { map.insert(stream_id.clone(), cancel.clone()); };
        }
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let app2 = app.clone();
        let sid = stream_id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::cloud::ops::list_stream(host, conn, bucket, prefix, sid.clone(), cap, cancel).await;
            let st = app2.state::<AppState>();
            if let Ok(mut map) = st.cloud_cancellations.lock() { map.remove(&sid); };
        });
        return Ok(serde_json::Value::String(stream_id));
    }
    if method == "__cloud_search_stream" {
        let op = "arbor.cloud.search_stream";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `stream_id`"))?.to_string();
        let pattern = params.get("pattern").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `pattern`"))?.to_string();
        let root_prefix = params.get("root_prefix").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        {
            let state = app.state::<AppState>();
            if let Ok(mut map) = state.cloud_cancellations.lock() { map.insert(stream_id.clone(), cancel.clone()); };
        }
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let app2 = app.clone();
        let sid = stream_id.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::cloud::ops::search_stream(host, conn, bucket, root_prefix, pattern, sid.clone(), cancel).await;
            let st = app2.state::<AppState>();
            if let Ok(mut map) = st.cloud_cancellations.lock() { map.remove(&sid); };
        });
        return Ok(serde_json::Value::String(stream_id));
    }
    if method == "__cloud_cancel" {
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        if let Ok(map) = state.cloud_cancellations.lock() {
            if let Some(flag) = map.get(stream_id) { flag.store(true, std::sync::atomic::Ordering::Relaxed); }
        };
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_is_cancelled" {
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).unwrap_or_default();
        let state = app.state::<AppState>();
        let cancelled = if let Ok(map) = state.cloud_cancellations.lock() {
            map.get(stream_id).map(|f| f.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false)
        } else { false };
        return Ok(serde_json::Value::Bool(cancelled));
    }
    if method == "__cloud_download" {
        let op = "arbor.cloud.download";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `path`"))?.to_string();
        let local = params.get("local").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `local`"))?.to_string();
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let id = tauri::async_runtime::block_on(crate::cloud::transfer::download(host, conn, bucket, path, std::path::PathBuf::from(local))).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::String(id));
    }
    if method == "__cloud_upload" {
        let op = "arbor.cloud.upload";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let path = params.get("path").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `path`"))?.to_string();
        let local = params.get("local").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `local`"))?.to_string();
        let overwrite = params.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false);
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let id = tauri::async_runtime::block_on(crate::cloud::transfer::upload(host, conn, bucket, path, std::path::PathBuf::from(local), overwrite)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::String(id));
    }
    if method == "__cloud_sync" {
        let op = "arbor.cloud.sync";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let remote_prefix = params.get("remote_prefix").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `remote_prefix`"))?.to_string();
        let local = params.get("local").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `local`"))?.to_string();
        let direction = params.get("direction").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `direction`"))?.to_string();
        let delete = params.get("delete").and_then(|v| v.as_bool()).unwrap_or(false);
        let dir = match direction.as_str() {
            "up" => crate::cloud::transfer::SyncDir::Up,
            "down" => crate::cloud::transfer::SyncDir::Down,
            other => return Err(format!("{op}: direction must be \"up\" or \"down\", got {other:?}")),
        };
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let id = tauri::async_runtime::block_on(crate::cloud::transfer::sync(host, conn, bucket, remote_prefix, std::path::PathBuf::from(local), dir, delete)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::String(id));
    }
    if method == "__cloud_download_many" {
        let op = "arbor.cloud.download_many";
        let conn: crate::cloud::types::CloudConnection = serde_json::from_value(
            params.get("conn").cloned().ok_or_else(|| format!("{op}: missing required `conn` table"))?,
        ).map_err(|e| format!("invalid conn: {e}"))?;
        let bucket = params.get("bucket").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `bucket`"))?.to_string();
        let local_dir = params.get("local_dir").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `local_dir`"))?.to_string();
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `stream_id`"))?.to_string();
        let parallel = params.get("parallel").and_then(|v| v.as_u64()).map(|n| n as usize);
        let op_label = params.get("op_label").and_then(|v| v.as_str()).map(|s| s.to_string());
        let paths: Vec<String> = params.get("paths").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
        if paths.is_empty() { return Err(format!("{op}: `paths` must contain at least one entry")); }
        let mut extra_steps: Vec<(String, String)> = Vec::new();
        if let Some(arr) = params.get("extra_steps").and_then(|v| v.as_array()) {
            for v in arr {
                let k = v.get("key").and_then(|x| x.as_str()).unwrap_or_default().to_string();
                let l = v.get("label").and_then(|x| x.as_str()).unwrap_or_default().to_string();
                if !k.is_empty() { extra_steps.push((k, l)); }
            }
        }
        let keep_open = params.get("keep_open").and_then(|v| v.as_bool()).unwrap_or(false);
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let job_id = tauri::async_runtime::block_on(crate::cloud::transfer::download_many(
            host, conn, bucket, paths, std::path::PathBuf::from(local_dir),
            parallel.unwrap_or(4).clamp(1, 16),
            op_label.unwrap_or_else(|| "Downloading items".to_string()),
            stream_id.clone(), extra_steps, keep_open,
        )).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::String(job_id));
    }
    if method == "__cloud_concat_files" {
        let op = "arbor.cloud.concat_files";
        let output = params.get("output").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `output`"))?.to_string();
        let inputs: Vec<String> = params.get("inputs").and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
        if inputs.is_empty() { return Err(format!("{op}: `inputs` must contain at least one entry")); }
        let delete_inputs = params.get("delete_inputs").and_then(|v| v.as_bool()).unwrap_or(false);
        tauri::async_runtime::block_on(crate::cloud::ops::concat_files(inputs, output, delete_inputs)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_report_progress" {
        let op = "arbor.cloud.report_progress";
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `stream_id`"))?;
        let step = params.get("step").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `step`"))?;
        let status = params.get("status").and_then(|v| v.as_str());
        let detail = params.get("detail").and_then(|v| v.as_str());
        let op_id = format!("cloud-storage:op:{stream_id}");
        let kind = if status.is_some() { "update_step" } else { "set_current" };
        use tauri::Emitter;
        let _ = app.emit("arbor://plugin-operation-update", serde_json::json!({
            "id": op_id, "plugin": "cloud-storage", "kind": kind,
            "step": step, "status": status, "detail": detail,
        }));
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_report_done" {
        let op = "arbor.cloud.report_done";
        let stream_id = params.get("stream_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `stream_id`"))?.to_string();
        let ok = params.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        let summary = params.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());
        let error = params.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());
        let op_id = format!("cloud-storage:op:{stream_id}");
        use tauri::Emitter;
        let _ = app.emit("arbor://plugin-operation-finish", serde_json::json!({
            "id": op_id, "plugin": "cloud-storage", "summary": summary, "error": error,
        }));
        let state = app.state::<AppState>();
        let job_id = state.cloud_pending_ops.lock().ok().and_then(|mut m| m.remove(&stream_id));
        if let Some(job_id) = job_id {
            let cancelled = state.cloud_cancellations.lock().ok()
                .and_then(|m| m.get(&stream_id).cloned())
                .map(|f| f.load(std::sync::atomic::Ordering::Relaxed)).unwrap_or(false);
            if let Ok(mut jobs) = state.lock_jobs() {
                let status = if ok { crate::jobs::JobStatus::Completed { exit_code: 0 } }
                    else if cancelled { crate::jobs::JobStatus::Cancelled }
                    else { crate::jobs::JobStatus::Failed { error: error.clone().unwrap_or_else(|| "merge failed".into()) } };
                jobs.set_status(&job_id, status);
            }
            let final_err = if ok { None } else if cancelled { Some("cancelled".to_string()) } else { error.clone().or_else(|| Some("merge failed".into())) };
            let _ = app.emit("arbor://job-done", serde_json::json!({
                "job_id": job_id, "success": ok, "exit_code": if ok { 0 } else { -1 },
                "cancelled": cancelled, "error": final_err,
            }));
            if let Ok(mut map) = state.cloud_cancellations.lock() { map.remove(&job_id); map.remove(&stream_id); }
        }
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_pick_chunk_order" {
        let op = "arbor.cloud.pick_chunk_order";
        let action = params.get("action").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `action`"))?.to_string();
        let op_label = params.get("op_label").and_then(|v| v.as_str()).map(|s| s.to_string());
        let pname = params.get("plugin_name").and_then(|v| v.as_str()).unwrap_or("cloud-storage").to_string();
        let items = params.get("items").cloned().unwrap_or(serde_json::Value::Array(Vec::new()));
        let extra = params.get("extra").cloned().unwrap_or(serde_json::Value::Object(Default::default()));
        use tauri::Emitter;
        let _ = app.emit("arbor://cloud-chunk-order-open", serde_json::json!({
            "plugin_name": pname, "op_label": op_label, "action": action, "items": items, "extra": extra,
        }));
        return Ok(serde_json::Value::Null);
    }
    if method == "__cloud_oauth_start" {
        let op = "arbor.cloud.oauth_start";
        let secret_ref = params.get("secret_ref").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `secret_ref`"))?.to_string();
        let client_id = params.get("client_id").and_then(|v| v.as_str()).ok_or_else(|| format!("{op}: missing required field `client_id`"))?.to_string();
        let client_secret = params.get("client_secret").and_then(|v| v.as_str()).map(|s| s.to_string());
        let host = app.state::<AppState>().cloud_host().ok_or_else(|| format!("{op}: cloud host not ready"))?;
        let url = tauri::async_runtime::block_on(crate::cloud::oauth_google::start(host, secret_ref, client_id, client_secret)).map_err(|e| e.to_string())?;
        return Ok(serde_json::Value::String(url));
    }

    // Master plugin kill-switch persistence (reverse method). After the Phase-2
    // flip, corvus-be owns the live Corvus plugin host and serves the
    // `set_plugins_enabled` RPC, but the typed `AppConfig.plugins_enabled` flag +
    // its TOML writer live only in the shell. corvus-be round-trips the FLAG write
    // here: idempotent compare-and-save, returning whether anything changed
    // (`true` → corvus-be applies the runtime mutation; `false` → it short-circuits,
    // mirroring the shell's old early `return Ok(())`). Mirrors
    // `ipc/platform/plugin.rs::set_plugins_enabled`'s persistence block exactly.
    if method == "__set_plugins_enabled" {
        let enabled = params.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
        let st = app.state::<AppState>();
        let mut cfg = st.lock_config().map_err(|e| e.to_string())?;
        if cfg.plugins_enabled == enabled {
            return Ok(serde_json::json!(false));
        }
        cfg.plugins_enabled = enabled;
        if let Err(e) = crate::config::app_config::save(&cfg) {
            tracing::warn!("failed to persist plugins_enabled: {e}");
        }
        return Ok(serde_json::json!(true));
    }

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

/// Maps a `corvus_brp::prelude::BrpError` to the Lua single-shot error envelope,
/// mirroring `ns_shell/brp.rs::error_from_brp`. Used by the `__brp_connect` /
/// `__brp_call` proxy handlers in `host_dispatch`.
fn brp_error_envelope_value(e: corvus_brp::prelude::BrpError) -> serde_json::Value {
    use corvus_brp::prelude::BrpError;
    let err = match e {
        BrpError::Transport(m) => serde_json::json!({ "kind": "transport", "message": m }),
        BrpError::Status { status, body } => serde_json::json!({ "kind": "status", "message": format!("HTTP {status}: {body}"), "code": status as i64 }),
        BrpError::InvalidResponse(m) => serde_json::json!({ "kind": "invalid_response", "message": m }),
        BrpError::Rpc { code, message, data } => {
            let mut e = serde_json::json!({ "kind": "rpc", "message": message, "code": code });
            if let Some(d) = data { e["data"] = d; }
            e
        }
    };
    serde_json::json!({ "ok": false, "error": err })
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

/// Lazily spawn `corvus-be` and attach it to the router, **idempotently**.
/// Called when the Corvus product window opens
/// (`window::corvus::open_corvus_window`), off the main thread — the spawn blocks
/// on the child's first `Hello` frame, which must not stall the UI thread.
///
/// First call spawns the binary, reads its advertised methods, splices the
/// client into the shared OOP routing slot, then pushes the current config so the
/// backend self-detects git + loads its owned product config before the
/// AppShell's first BE-required `rpc` fires. Subsequent calls (the single Corvus
/// window re-summoned) are a no-op while the backend is alive. If the binary is
/// missing / the spawn fails, the backend stays detached and every corvus method
/// routes in-process — the same BE-required-method semantics as before (the user
/// builds `corvus-be`).
pub fn ensure_corvus_be(app: &AppHandle) {
    // Serialize concurrent triggers (launcher button + Command Palette can both
    // fire `open_corvus_window`) so we never spawn two backends; re-check
    // liveness inside the lock.
    static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = match SPAWN_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if split_broker::is_attached("corvus") {
        return; // backend already up — window is just being re-summoned
    }
    match spawn_corvus_be(app) {
        Some((child, methods)) => {
            tracing::info!(
                "corvus-be up (lazy): {} method(s) served out-of-process",
                methods.len()
            );
            split_broker::attach(
                "corvus",
                methods.into_iter().collect(),
                Arc::new(child) as Arc<dyn BrokerClient>,
            );
            // Now that the backend is listening, hand it the runtime config it
            // can't resolve itself (git path + portable dir, owned-config path,
            // registry/workspace paths) so it's ready before the first repo opens.
            sync_config(&app.state::<AppState>());
        }
        None => {
            tracing::info!("corvus-be not available — Corvus running in-process");
        }
    }
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
    let app_for_host = app.clone();
    let app_for_disc = app.clone();
    match ChildClient::spawn(
        cmd,
        move |topic, payload| {
            use tauri::Emitter;
            let _ = app_for_events.emit(&topic, payload);
        },
        move |method, params| host_dispatch(&app_for_host, method, params),
        move || {
            // The git backend process died (crash / kill). Detach it so the
            // router stops routing to a dead pipe (corvus methods fall back to
            // the in-process loopback → UnknownMethod for the BE-required ones),
            // then surface a fatal state: there is no live respawn yet, so the
            // Corvus window shows a blocking overlay asking the user to restart.
            use tauri::Emitter;
            split_broker::detach("corvus");
            let _ = app_for_disc.emit("arbor://corvus-be-down", ());
        },
    ) {
        Ok(pair) => Some(pair),
        Err(e) => {
            tracing::warn!("failed to spawn corvus-be ({e}) — staying in-process");
            None
        }
    }
}

/// Lazily spawn `merula-be` and attach it to the router, **idempotently** — the
/// merula twin of [`ensure_corvus_be`]. Called when the Merula product window
/// opens (`window::merula::open_merula_window`), off the main thread — the spawn
/// blocks on the child's first `Hello` frame, which must not stall the UI thread.
///
/// First call spawns the binary, reads its advertised methods, and splices the
/// client into the `merula` slot of the shared OOP routing map. Subsequent calls
/// (the single Merula window re-summoned) are a no-op while the backend is alive.
/// **No `sync_config`**: `merula-be` resolves its own `merula_config_dir()` /
/// `merula_data_dir()` itself once `init_active_profile()` has run (it owns no
/// shell-pushed config). If the binary is missing / the spawn fails, the backend
/// stays detached and every `merula` rpc method routes to the loopback →
/// `UnknownMethod` (the FE shows the down overlay).
pub fn ensure_merula_be(app: &AppHandle) {
    // Serialize concurrent triggers (launcher button + Command Palette can both
    // fire `open_merula_window`) so we never spawn two backends; re-check liveness
    // inside the lock. A SEPARATE lock from corvus's so the two backends' spawns
    // never contend.
    static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = match SPAWN_LOCK.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if split_broker::is_attached("merula") {
        return; // backend already up — window is just being re-summoned
    }
    match spawn_merula_be(app) {
        Some((child, methods)) => {
            tracing::info!(
                "merula-be up (lazy): {} method(s) served out-of-process",
                methods.len()
            );
            split_broker::attach(
                "merula",
                methods.into_iter().collect(),
                Arc::new(child) as Arc<dyn BrokerClient>,
            );
            // No config push: merula-be owns and resolves its own config/data dirs.
        }
        None => {
            tracing::info!("merula-be not available — Merula running in-process");
        }
    }
}

fn spawn_merula_be(app: &AppHandle) -> Option<(ChildClient, Vec<String>)> {
    use crate::process_ext::NoWindowExt;
    use crate::window::merula::MERULA_WINDOW_LABEL;

    let bin = match backend_binary(app, "merula-be") {
        Some(b) => b,
        None => {
            tracing::info!(
                "merula-be binary not found (backends/ resource or beside the launcher) — staying in-process"
            );
            return None;
        }
    };

    let mut cmd = std::process::Command::new(&bin);
    cmd.no_window(); // no console popup on Windows; stdio piping is unaffected

    let app_for_events = app.clone();
    let app_for_host = app.clone();
    let app_for_disc = app.clone();
    match ChildClient::spawn(
        cmd,
        move |topic, payload| {
            // CRITICAL: re-emit merula-be's push events SCOPED TO THE MERULA
            // WINDOW only — never the global `app.emit` corvus-be uses. merula's
            // `merula:*` topics (meters / transport / active_haps / diagnostics)
            // tick at audio rate; broadcasting them app-wide would flood the
            // launcher + Corvus windows with another product's telemetry. This
            // mirrors the in-process `merula::events::emit` (`emit_to`).
            use tauri::Emitter;
            let _ = app_for_events.emit_to(MERULA_WINDOW_LABEL, &topic, payload);
        },
        move |method, params| host_dispatch(&app_for_host, method, params),
        move || {
            // The audio backend process died (crash / kill). Detach it so the
            // router stops routing to a dead pipe (merula methods fall back to the
            // loopback → UnknownMethod), then surface a fatal state: there is no
            // live respawn yet, so the Merula window shows a blocking overlay
            // asking the user to restart.
            split_broker::detach("merula");
            use tauri::Emitter;
            let _ = app_for_disc.emit_to(MERULA_WINDOW_LABEL, "arbor://merula-be-down", ());
        },
    ) {
        Ok(pair) => Some(pair),
        Err(e) => {
            tracing::warn!("failed to spawn merula-be ({e}) — staying in-process");
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

/// Push a tab's repo path (and the app-config slices) to `corvus-be`, so its
/// out-of-process handlers can resolve the tab without the shell's `RepoManager`.
/// Call on repo open. corvus-be self-detects its git binary now (it is no longer
/// pushed the resolved program).
///
/// **Best-effort**: when `corvus-be` isn't running the `__repo_register` method
/// isn't advertised, so the call routes to the in-process loopback, comes back
/// `UnknownMethod`, and is dropped here — exactly what we want (nothing to sync).
pub fn sync_repo_open(state: &AppState, tab_id: &str, path: &str) {
    sync_config(state);
    let _ = dispatch_rpc(
        state,
        "corvus",
        "__repo_register",
        serde_json::json!({ "tab_id": tab_id, "path": path }),
    );
}

/// Push the runtime values `corvus-be` can't resolve on its own: the absolute,
/// profile-aware PATH of its owned `corvus/config.toml` (so it can load/save the
/// git-product config sections it now owns — recovery, gitflow, diff, status,
/// cache, …), the resolved `git` program + portable dir for self-detection, the
/// repo registry, and the workspace/worktree-link file paths. Called on repo
/// open (alongside the git program) and whenever the user changes a relevant
/// setting. **Best-effort**: when `corvus-be` isn't running the `__set_config`
/// method routes to the in-process loopback, returns `UnknownMethod`, and is
/// dropped. corvus-be owns and reads the config-content sections from the file
/// directly now, so only the path is handed over (not the values). The per-repo
/// `.arbor/config.toml` overrides are NOT pushed — corvus-be reads those from the
/// workdir it opens.
pub fn sync_config(state: &AppState) {
    let cfg = crate::config::app_config::load().unwrap_or_default();
    // corvus-be OWNS its product config (`corvus/config.toml`: diff, graph,
    // gitflow, cache, ticket_links, issues, mr, status, recovery,
    // missing_projects, pipelines, studio, commit, branches) and reads those
    // sections from the file directly, so they are no longer pushed. The shell
    // only hands over the profile-resolved absolute PATH of that file — corvus-be
    // is a separate process and can't resolve the active profile itself. A
    // profile switch re-pushes a new path → corvus-be loads from it on next access.
    push_config_section(
        state,
        "corvus_config_path",
        &crate::config::corvus_read::corvus_config_path().to_string_lossy().to_string(),
    );
    // Git CLI: the configured executable_path override + the absolute, profile-
    // resolved PortableGit dir. corvus-be self-detects from these — it can't
    // resolve the active profile to recompute the portable dir itself.
    push_config_section(
        state,
        "git",
        &serde_json::json!({
            "executable_path": cfg.git.executable_path,
            "portable_dir": crate::git_cli::portable_dir().display().to_string(),
        }),
    );
    // Not an app-config slice: the absolute path of the profile's
    // `linked_worktrees.toml`. corvus-be is a separate process and can't compute
    // the profile-aware path itself, so the shell (which owns the active profile)
    // hands it over; corvus-be (re)loads its worktree-link registry from it. A
    // profile switch re-pushes a new path → corvus-be reloads on next access.
    let lw_path = crate::linked_worktrees::links_file_path().to_string_lossy().to_string();
    push_config_section(state, "worktree_links_path", &lw_path);
    // Profile-aware absolute paths of the workspace-subsystem files corvus-be
    // owns (ADR-1: repo registry + workspace store + per-workspace tab snapshots).
    // corvus-be is a separate process and can't resolve the active profile, so the
    // shell hands over the paths; corvus-be (re)loads each on access. A profile
    // switch re-pushes new paths → corvus-be reloads on next access.
    push_config_section(
        state,
        "repo_registry_path",
        &crate::workspace::registry::registry_path().to_string_lossy().to_string(),
    );
    push_config_section(
        state,
        "workspaces_path",
        &crate::workspace::store::store_path().to_string_lossy().to_string(),
    );
    push_config_section(
        state,
        "workspace_state_dir",
        &crate::workspace::snapshot::snapshot_dir().to_string_lossy().to_string(),
    );
    // repo_id → {path, display_name} for every known repo: the worktree-link
    // checkout-sync orchestrator resolves member repo_ids to paths through this
    // (`CorvusState` only tracks open tabs by `tab_id`, not the repo registry).
    let repos = state.lock_repo_registry().map(|r| r.list()).unwrap_or_default();
    push_config_section(state, "repo_registry", &repos);
}

/// On a live profile switch, push the new active profile to a running `corvus-be`
/// and reload its plugin host, so the target profile's plugin set loads and
/// re-emits its contributions to the Corvus window. Ordered + best-effort:
/// `__set_plugin_profile` repoints the backend's profile cell + marketplace plugin
/// root, then `reload_plugins` rescans and re-announces (emitting
/// `arbor://plugins-reloaded` / contribution events the shell re-forwards to the
/// FE). A no-op when `corvus-be` isn't running — the methods aren't advertised so
/// the calls drop, and it adopts the new profile from the on-disk pointer the next
/// time it spawns.
pub fn reload_corvus_plugins(state: &AppState) {
    let profile = arbor_core::prelude::active_profile();
    let _ = dispatch_rpc(
        state,
        "corvus",
        "__set_plugin_profile",
        serde_json::json!({ "profile": profile }),
    );
    let _ = dispatch_rpc(state, "corvus", "reload_plugins", serde_json::json!({}));
}

/// Relay a plugin hook fired **shell-side** to the product backend where the
/// target plugin now runs. After the plugin-relocation flip, universal plugins
/// (e.g. `cloud-storage`) load in `corvus-be`, so a hook the shell raises for them
/// — the cloud stream callbacks (`cloud-storage:list-chunk`), OAuth-done, transfer
/// job-done / progress — must be forwarded there or the plugin never sees it and
/// the UI hangs (the "Loading…" stall). This routes through `corvus-be`'s
/// `fire_plugin_action` (the exact cross-process twin of the shell's plugin-
/// targeted `fire_on`): same plugin, same callback name, same payload. Best-effort
/// — the method is advertised only while `corvus-be` runs, so the call drops when
/// it isn't. `payload_json` is the already-serialized hook payload, handed over
/// verbatim as the context.
pub fn fire_plugin_hook_on_backends(app: &AppHandle, plugin: &str, hook: &str, payload_json: &str) {
    let state = app.state::<AppState>();
    let _ = dispatch_rpc(
        &state,
        "corvus",
        "fire_plugin_action",
        serde_json::json!({
            "plugin_name":  plugin,
            "action":       hook,
            "context_json": payload_json,
        }),
    );
}

/// Route one BRP watch SSE event back to the `corvus-be` plugin that owns the
/// parked `arbor.brp.watch` callback. The SSE stream runs shell-side (the
/// `BrpRegistry` lives in `AppState.brp`), but the watch closure lives in the
/// backend VM under `__arbor_hooks__[<callback_id>]`, so each event must be pushed
/// across via `corvus-be`'s `invoke_plugin_callback` RPC — the cross-process twin
/// of firing the parked closure in-process. Fire-and-forget; best-effort (the
/// method is advertised only while `corvus-be` runs, so the call drops when it
/// isn't). `payload_json` is the already-serialized `watch_event_to_payload`
/// envelope, handed over verbatim as the callback context.
pub fn invoke_plugin_callback_on_backend(
    app: &AppHandle,
    plugin: &str,
    callback_id: &str,
    payload_json: &str,
) {
    let state = app.state::<AppState>();
    let _ = dispatch_rpc(
        &state,
        "corvus",
        "invoke_plugin_callback",
        serde_json::json!({
            "plugin_name": plugin,
            "callback_id": callback_id,
            "context_json": payload_json,
        }),
    );
}

/// Drop a parked `corvus-be` plugin callback once its `arbor.brp.watch`
/// subscription is torn down (explicit `unwatch` or stream-end). Mirrors
/// [`invoke_plugin_callback_on_backend`] but drives the teardown RPC so the
/// backend VM frees the closure from `__arbor_hooks__`. Fire-and-forget;
/// best-effort.
fn remove_plugin_callback_on_backend(state: &AppState, plugin: &str, callback_id: &str) {
    let _ = dispatch_rpc(
        state,
        "corvus",
        "remove_plugin_callback",
        serde_json::json!({
            "plugin_name": plugin,
            "callback_id": callback_id,
        }),
    );
}

/// Build the Lua-shaped watch envelope for one SSE event — byte-for-byte the
/// shell's old `ns_shell/brp.rs::watch_event_to_payload`. Each variant maps to a
/// single `{ ok, event, … }` table the plugin's watch callback receives.
fn watch_event_to_payload(event: &corvus_brp::prelude::WatchEvent) -> serde_json::Value {
    use corvus_brp::prelude::WatchEvent;
    match event {
        WatchEvent::Open => serde_json::json!({ "ok": true, "event": "open" }),
        WatchEvent::Data(v) => serde_json::json!({ "ok": true, "event": "data", "result": v }),
        WatchEvent::Close => serde_json::json!({ "ok": true, "event": "close" }),
        WatchEvent::Error(msg) => serde_json::json!({
            "ok": false,
            "event": "error",
            "error": { "kind": "transport", "message": msg },
        }),
        WatchEvent::RpcError { code, message, data } => {
            let mut err = serde_json::json!({
                "kind": "rpc",
                "message": message,
                "code": code,
            });
            if let Some(d) = data {
                err["data"] = d.clone();
            }
            serde_json::json!({ "ok": false, "event": "error", "error": err })
        }
    }
}

/// Serialize one app-config slice and push it into `corvus-be`'s config bag.
fn push_config_section<T: serde::Serialize>(state: &AppState, section: &str, value: &T) {
    if let Ok(value) = serde_json::to_value(value) {
        let _ = dispatch_rpc(
            state,
            "corvus",
            "__set_config",
            serde_json::json!({ "section": section, "value": value }),
        );
    }
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
