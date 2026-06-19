//! `corvus-be` — the headless git backend process for Model D.
//!
//! Stage 1 proved the process boundary end to end (spawn, framed-stdio
//! handshake, request/response, event push, error wire-format) with a self-test
//! method set. Stage 2 moves the git domains onto it: each domain's handler
//! functions live in their own module here, auto-advertised via `Hello` and
//! auto-routed out-of-process by the shell's `SplitBroker`, once their git
//! dependencies are extracted into the shared `corvus-git` crate. **bisect** and
//! **stash** are served so far (reset next). See `docs/corvus-be-bringup.md`.
//!
//! It owns a [`CorvusState`] (the shell pushes the open tabs' repo paths + the
//! resolved git program into it); handlers resolve a `tab_id` to a path and run
//! the shared `corvus-git` logic. The shell re-emits this process's events to the
//! FE and fires any owed plugin hooks shell-side after the call returns.
//!
//! **stdout is the protocol channel** — all logs go to stderr.

use std::any::Any;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::{serve_stdio, EventSink, FrameEventSink, FrameHostCaller, HostCaller, SharedWriter};
use arbor_plugin_core::prelude::PluginHost;
use corvus_core::prelude::CorvusState;
use corvus_plugin::prelude::{build_hook_dispatcher, corvus_be_api_installer, CorvusBeAppCtx};

// Domain handler modules — their `#[arbor_rpc::handler]`s self-register via
// inventory, so `arbor_rpc::registry()` collects them and `Hello` advertises
// them. The shell pushes repo paths to `repo_registry`; `bisect` and `stash`
// are the git domains served out-of-process so far.
mod avatar;
mod bisect;
mod branch;
mod ci;
mod diff;
mod gitflow;
mod graph;
mod issues;
mod jobs;
mod linked_worktree;
mod merge;
mod mr;
mod notes;
mod provider;
mod rebase;
mod recovery;
mod reflog;
mod remote;
mod repo;
mod repo_browser;
mod repo_ops;
mod repo_registry;
mod reset;
mod search;
mod security;
mod stage;
mod stash;
mod stats;
mod status;
mod submodule;
mod tickets;
mod worktree;
mod worktree_links;

// ── Self-test handlers (Stage 1) ────────────────────────────────────────────
// Plain `#[arbor_rpc::handler]`s, exactly like the shell-side ones — the context
// is `&CorvusState` (downcast from `&dyn Any` by the generated thunk). They
// register via `inventory`, so `arbor_rpc::registry()` collects them and `Hello`
// advertises them by name.

/// Liveness round-trip: `rpc("corvus", "be_ping", {})` → `"pong"`.
#[arbor_rpc::handler]
fn be_ping(_ctx: &CorvusState) -> Result<String, String> {
    Ok("pong".to_string())
}

/// Echo — proves argument decode across the boundary.
#[arbor_rpc::handler]
fn be_echo(_ctx: &CorvusState, message: String) -> Result<String, String> {
    Ok(message)
}

/// Push-event proof: emits `arbor://corvus-be-pong` back through the sink, which
/// the shell re-emits to the FE. Returns immediately.
#[arbor_rpc::handler]
fn be_emit(ctx: &CorvusState, note: Option<String>) -> Result<(), String> {
    ctx.emit(
        "arbor://corvus-be-pong",
        serde_json::json!({ "from": "corvus-be", "note": note }),
    );
    Ok(())
}

/// Reverse-channel proof (`docs/reverse-channel.md`): resolve a credential
/// session for `account` by calling **back** to the shell — which holds the
/// keyring + `VaultSessionProvider` — and return only the resolved `base_url`,
/// never the token. Exercises the whole backend→shell→keyring chain end to end.
/// e.g. `rpc("corvus", "be_session_probe", { "account": "linear" })` with a
/// connected Linear account → `"https://api.linear.app/graphql"`.
///
/// Synchronous on purpose: `host_call` blocks on the shell's reply, delivered by
/// the serve loop's reader thread while this handler is parked on its worker —
/// the reentrancy the reverse channel is built for.
#[arbor_rpc::handler]
fn be_session_probe(ctx: &CorvusState, account: String) -> Result<String, String> {
    let session = ctx.host_call("__session", serde_json::json!(account))?;
    let base = session.get("base_url").and_then(|b| b.as_str()).unwrap_or_default();
    Ok(base.to_string())
}

fn main() {
    // stdout carries frames; logs go to stderr.
    let stdout: SharedWriter = Arc::new(Mutex::new(io::stdout()));

    // Event egress: a frame sink writing onto the same stdout the serve loop uses
    // (the mutex serializes the two). This is what `CorvusState` holds — handlers
    // emit through it exactly as in-process, but it crosses the process boundary.
    let sink: Arc<dyn EventSink> = Arc::new(FrameEventSink::new(Arc::clone(&stdout)));

    // Reverse channel (`docs/reverse-channel.md`): the backend's `HostCaller`,
    // writing `HostRequest` frames on the same stdout the serve loop routes
    // `HostResponse`s back through. Handlers reach it via `state.host_call(...)`.
    let host = FrameHostCaller::new(Arc::clone(&stdout));

    // Async runtime, built first: the plugin host's `AppCtx` captures a handle to
    // spawn background plugin work (the boot thread has no ambient reactor), and
    // the async issue-tracker handlers `block_on` it. The serve loop dispatches
    // each request on its own worker thread, so concurrent `block_on`s land here
    // — a multi-thread runtime is required.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("corvus-be: failed to build tokio runtime");

    // Plugin host co-located with the git handlers (plugin-relocation Wave 0):
    // the hooks the OOP handlers fire reach plugins *here*, in the process that
    // runs the git logic, instead of being dropped. The headless installer
    // publishes the host-pure `arbor.*` surface only — the git/product `ns_shell`
    // namespaces arrive in Wave 1, so a hook that calls one gets a clear error,
    // never a silent drop. Schedulers are not started here yet (Wave 1+).
    let plugin_host = Arc::new(Mutex::new(PluginHost::new()));
    {
        let mut h = plugin_host.lock().expect("corvus-be: plugin host lock poisoned at boot");
        h.set_app_ctx(Arc::new(CorvusBeAppCtx::new(Arc::clone(&sink), rt.handle().clone())));
        h.set_api_installer(corvus_be_api_installer());
    }
    let hooks = Arc::new(build_hook_dispatcher(&plugin_host));
    if let Err(e) = plugin_host
        .lock()
        .expect("corvus-be: plugin host lock poisoned at boot")
        .reload()
    {
        eprintln!("corvus-be: plugin reload failed: {e}");
    }

    // The state handed to every handler: event egress + the hook broker bound to
    // the host above + the reverse channel back to the shell.
    let state = CorvusState::new(sink)
        .with_hooks(hooks)
        .with_host_caller(Arc::clone(&host) as Arc<dyn HostCaller>);

    // The issue-tracker registry resolves credentials over the reverse channel
    // (the shell holds the keyring) — wire it before serving.
    issues::init(Arc::clone(&host) as Arc<dyn HostCaller>);

    // The git-provider registry (repo-browser + the REST cohort) resolves
    // credentials over the same reverse channel — seed it before serving.
    provider::init(Arc::clone(&host) as Arc<dyn HostCaller>);

    // Two registries collected from every `#[arbor_rpc::handler]` linked into
    // this binary: sync (git domains + self-test) and async (issue trackers).
    // They are disjoint — a handler is one or the other.
    let sync_reg = arbor_rpc::registry();
    let async_reg = arbor_rpc::async_registry_for("");
    let mut methods: Vec<String> = sync_reg
        .keys()
        .chain(async_reg.keys())
        .map(|s| s.to_string())
        .collect();
    methods.sort();
    methods.dedup();
    eprintln!("corvus-be: ready, serving {} method(s): {:?}", methods.len(), methods);

    let dispatch = move |method: &str, params: serde_json::Value| -> Result<serde_json::Value, String> {
        if let Some(call) = sync_reg.get(method) {
            return call(&state as &dyn Any, params);
        }
        if let Some(acall) = async_reg.get(method) {
            return rt.block_on(acall(&state as &dyn Any, params));
        }
        Err(format!("unknown method: {method}"))
    };

    if let Err(e) = serve_stdio(io::stdin().lock(), stdout, methods, host, dispatch) {
        eprintln!("corvus-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
