//! `SplitBroker` — routes each program method to the right place during the
//! in-process → out-of-process migration, with a **lazily-attached** backend.
//!
//! A method the running product backend (`corvus-be` / `merula-be`) advertised
//! (in its `Hello`) goes to the child process; everything else stays in-process
//! on the `LoopbackBroker`. Moving a handler into the backend therefore flips its
//! routing automatically — no per-method edits here. See
//! `docs/corvus-be-bringup.md`.
//!
//! The out-of-process channel is **not** wired at construction: the backend is
//! spawned lazily when its product window first opens (see
//! [`crate::ipc::ensure_corvus_be`] / [`crate::ipc::ensure_merula_be`]), so the
//! broker starts loopback-only and the child + its advertised method set are
//! spliced in (and torn back out on disconnect) at runtime through [`attach`] /
//! [`detach`]. Until a child is attached every method routes in-process — the
//! correct fallback both before the backend is up and after it dies.
//!
//! **Per-program slots.** Each registered [`SplitBroker`] carries its program
//! label (`"corvus"` / `"merula"`); the live OOP channels live in a process-wide
//! map keyed by that label, so a Corvus backend and a Merula backend attach and
//! route independently. `merula-be` has no in-process handlers, so when it is
//! detached every `merula` method falls through to the loopback → `UnknownMethod`
//! (the FE shows the down overlay) — the correct behaviour.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::{Arc, LazyLock, RwLock};

use arbor_ipc::prelude::{BrokerClient, Bytes, IpcError};

/// One program's live out-of-process channel: the advertised method set + the
/// client that reaches its backend. Present once the backend is spawned, removed
/// when it disconnects.
struct Oop {
    methods: HashSet<String>,
    child: Arc<dyn BrokerClient>,
}

/// Process-wide OOP channels, keyed by program label (`"corvus"` / `"merula"`),
/// shared by the registered [`SplitBroker`]s (for routing) and `crate::ipc` (for
/// `is_oop_method` + attach/detach across each backend's lifecycle). One backend
/// process per product, so one slot per program.
static OOP: LazyLock<RwLock<HashMap<&'static str, Oop>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Attach a freshly-spawned backend for `program`: route its advertised `methods`
/// to `child` from now on. Replaces any prior channel for that program.
pub fn attach(program: &'static str, methods: HashSet<String>, child: Arc<dyn BrokerClient>) {
    if let Ok(mut g) = OOP.write() {
        g.insert(program, Oop { methods, child });
    }
}

/// Detach `program`'s backend (it died or was shut down): every method falls back
/// to the in-process loopback. Dropping the stored client closes the pipe / kills
/// the child.
pub fn detach(program: &str) {
    if let Ok(mut g) = OOP.write() {
        g.remove(program);
    }
}

/// Whether a backend is currently attached for `program` (used to make the lazy
/// spawn idempotent).
pub fn is_attached(program: &str) -> bool {
    OOP.read().map(|g| g.contains_key(program)).unwrap_or(false)
}

/// Whether `method` is currently served out-of-process by `program`'s attached
/// backend. Drives `crate::ipc::is_oop_method`.
pub fn serves(program: &str, method: &str) -> bool {
    OOP.read()
        .map(|g| g.get(program).is_some_and(|o| o.methods.contains(method)))
        .unwrap_or(false)
}

/// Broker registered once per program. Holds the program label + its in-process
/// loopback; the out-of-process channel lives in the shared [`OOP`] map so it can
/// be attached/detached over the backend's lifetime without rebuilding the router.
pub struct SplitBroker {
    program: &'static str,
    loopback: Arc<dyn BrokerClient>,
}

impl SplitBroker {
    pub fn new(program: &'static str, loopback: Arc<dyn BrokerClient>) -> Self {
        Self { program, loopback }
    }
}

impl BrokerClient for SplitBroker {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
        // Clone the child Arc out under the read lock, then release it before the
        // blocking IPC round-trip — so a concurrent `detach()` (on backend death)
        // never waits on an in-flight call.
        let child = match OOP.read() {
            Ok(g) => match g.get(self.program) {
                Some(o) if o.methods.contains(method) => Some(o.child.clone()),
                _ => None,
            },
            Err(_) => None,
        };
        match child {
            Some(c) => c.call(method, params),
            None => self.loopback.call(method, params),
        }
    }
}
