//! `SplitBroker` — routes each `corvus` method to the right place during the
//! in-process → out-of-process migration, with a **lazily-attached** backend.
//!
//! A method the running `corvus-be` advertised (in its `Hello`) goes to the
//! child process; everything else stays in-process on the `LoopbackBroker`.
//! Moving a handler into `corvus-be` therefore flips its routing automatically —
//! no per-method edits here. See `docs/corvus-be-bringup.md`.
//!
//! The out-of-process channel is **not** wired at construction: `corvus-be` is
//! spawned lazily when the Corvus product window first opens (see
//! [`crate::ipc::ensure_corvus_be`]), so the broker starts loopback-only and the
//! child + its advertised method set are spliced in (and torn back out on
//! disconnect) at runtime through [`attach`] / [`detach`]. Until a child is
//! attached every method routes in-process — the correct fallback both before
//! the backend is up and after it dies.

use std::collections::HashSet;
use std::sync::{Arc, LazyLock, RwLock};

use arbor_ipc::prelude::{BrokerClient, Bytes, IpcError};

/// The live out-of-process channel: the advertised method set + the client that
/// reaches `corvus-be`. `None` until the backend is spawned, reset to `None`
/// when it disconnects.
struct Oop {
    methods: HashSet<String>,
    child: Arc<dyn BrokerClient>,
}

/// Process-wide OOP channel shared by the registered [`SplitBroker`] (for
/// routing) and `crate::ipc` (for `is_oop_method` + attach/detach across the
/// backend lifecycle). A single `corvus-be` serves every Corvus tab, so one slot.
static OOP: LazyLock<RwLock<Option<Oop>>> = LazyLock::new(|| RwLock::new(None));

/// Attach a freshly-spawned `corvus-be`: route its advertised `methods` to
/// `child` from now on. Replaces any prior channel.
pub fn attach(methods: HashSet<String>, child: Arc<dyn BrokerClient>) {
    if let Ok(mut g) = OOP.write() {
        *g = Some(Oop { methods, child });
    }
}

/// Detach the backend (it died or was shut down): every method falls back to the
/// in-process loopback. Dropping the stored client closes the pipe / kills the
/// child.
pub fn detach() {
    if let Ok(mut g) = OOP.write() {
        *g = None;
    }
}

/// Whether a backend is currently attached (used to make the lazy spawn
/// idempotent).
pub fn is_attached() -> bool {
    OOP.read().map(|g| g.is_some()).unwrap_or(false)
}

/// Whether `method` is currently served out-of-process by the attached backend.
/// Drives `crate::ipc::is_oop_method`.
pub fn serves(method: &str) -> bool {
    OOP.read()
        .map(|g| g.as_ref().is_some_and(|o| o.methods.contains(method)))
        .unwrap_or(false)
}

/// Broker registered once for the `corvus` program. Holds only the in-process
/// loopback; the out-of-process channel lives in the shared [`OOP`] slot so it
/// can be attached/detached over the backend's lifetime without rebuilding the
/// router.
pub struct SplitBroker {
    loopback: Arc<dyn BrokerClient>,
}

impl SplitBroker {
    pub fn new(loopback: Arc<dyn BrokerClient>) -> Self {
        Self { loopback }
    }
}

impl BrokerClient for SplitBroker {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
        // Clone the child Arc out under the read lock, then release it before the
        // blocking IPC round-trip — so a concurrent `detach()` (on backend death)
        // never waits on an in-flight call.
        let child = match OOP.read() {
            Ok(g) => match g.as_ref() {
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
