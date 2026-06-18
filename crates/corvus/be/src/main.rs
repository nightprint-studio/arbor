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
use corvus_core::prelude::CorvusState;

// Domain handler modules — their `#[arbor_rpc::handler]`s self-register via
// inventory, so `arbor_rpc::registry()` collects them and `Hello` advertises
// them. The shell pushes repo paths to `repo_registry`; `bisect` and `stash`
// are the git domains served out-of-process so far.
mod bisect;
mod repo_registry;
mod stash;

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
    let state = CorvusState::new(sink).with_host_caller(Arc::clone(&host) as Arc<dyn HostCaller>);

    // The registry is collected from every `#[arbor_rpc::handler]` linked into
    // this binary (just the self-test set today; git handlers in Stage 2).
    let registry = arbor_rpc::registry();
    let mut methods: Vec<String> = registry.keys().map(|s| s.to_string()).collect();
    methods.sort();
    eprintln!("corvus-be: ready, serving {} method(s): {:?}", methods.len(), methods);

    let dispatch = move |method: &str, params: serde_json::Value| -> Result<serde_json::Value, String> {
        match registry.get(method) {
            Some(call) => call(&state as &dyn Any, params),
            None => Err(format!("unknown method: {method}")),
        }
    };

    if let Err(e) = serve_stdio(io::stdin().lock(), stdout, methods, host, dispatch) {
        eprintln!("corvus-be: serve loop ended with error: {e}");
        std::process::exit(1);
    }
    // Clean EOF: the shell exited.
    let _ = io::stderr().flush();
}
