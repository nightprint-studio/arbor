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
//! route independently.
//!
//! **Hybrid vs pure-out-of-process.** A product mid-migration (`corvus`) still has
//! some handlers in this shell, so an unrouted method legitimately falls to its
//! in-process [`LoopbackBroker`] — build it with [`SplitBroker::new`]. A product
//! with NO in-process handlers (`merula`, `sitta`) has no such fallback: its
//! methods belong to its backend and nowhere else. Build it with
//! [`SplitBroker::pure_oop`], and the router reports the real situation instead of
//! a catch-all "unknown method":
//!   * backend not attached → [`IpcError::BackendNotRunning`] (the process isn't up)
//!   * attached but method not advertised → [`IpcError::UnknownMethod`] (typo / not
//!     implemented)
//!
//! (A genuinely unregistered product is caught one level up by the router as
//! `RouterError::UnknownProduct`, before any `SplitBroker` is consulted.)

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, RwLock};

use arbor_ipc::prelude::{BrokerClient, Bytes, IpcError};

/// One program's live out-of-process channel: the advertised method set + the
/// client that reaches its backend. Present once the backend is spawned, removed
/// when it disconnects. `gen` is the spawn generation (see [`next_gen`]) — it lets
/// a late disconnect of an OLD child be told apart from the CURRENT one in logs.
struct Oop {
    gen: u64,
    methods: HashSet<String>,
    child: Arc<dyn BrokerClient>,
}

/// Monotonic spawn-generation counter. Each backend spawn takes one via
/// [`next_gen`]; it tags the attached entry and is captured by that child's
/// disconnect callback, so rapid open/stop cycles (which interleave a dying
/// child's teardown with a freshly-spawned one) are distinguishable in the logs.
static NEXT_GEN: AtomicU64 = AtomicU64::new(1);

/// Allocate a fresh spawn generation for a backend about to come up.
pub fn next_gen() -> u64 {
    NEXT_GEN.fetch_add(1, Ordering::Relaxed)
}

/// Process-wide OOP channels, keyed by program label (`"corvus"` / `"merula"`),
/// shared by the registered [`SplitBroker`]s (for routing) and `crate::ipc` (for
/// `is_oop_method` + attach/detach across each backend's lifecycle). One backend
/// process per product, so one slot per program.
static OOP: LazyLock<RwLock<HashMap<&'static str, Oop>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Attach a freshly-spawned backend for `program`: route its advertised `methods`
/// to `child` from now on. Replaces any prior channel for that program.
///
/// Any displaced prior client is dropped **after** the write lock is released —
/// dropping a `ChildClient` blocks in the child's `kill()`+`wait()`, which must
/// never run while the global routing lock is held (it would freeze every other
/// product's `call` / `is_attached`). In practice the lazy-spawn `is_attached`
/// guard means there's nothing to displace, but the ordering is load-bearing.
pub fn attach(program: &'static str, gen: u64, methods: HashSet<String>, child: Arc<dyn BrokerClient>) {
    let n_methods = methods.len();
    let displaced = {
        let mut g = match OOP.write() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.insert(program, Oop { gen, methods, child })
        // write lock released here ↓
    };
    match displaced.as_ref() {
        Some(prev) => tracing::warn!(
            "split_broker::attach({program}) gen={gen} ({n_methods} methods) DISPLACED a live gen={} — double-attach race",
            prev.gen
        ),
        None => tracing::info!("split_broker::attach({program}) gen={gen} ({n_methods} methods)"),
    }
    drop(displaced); // potential blocking child teardown, lock-free
}

/// Detach `program`'s backend (it died or was shut down): every method falls back
/// to the in-process loopback. Dropping the stored client closes the pipe / kills
/// the child.
///
/// Two hazards, both handled here so no caller has to remember:
///   1. The map entry is removed under a **brief** `OOP.write()` and the lock is
///      released immediately — so `is_attached` flips to `false` synchronously
///      (a re-open of the same product right after a close spawns a fresh backend
///      instead of racing a half-removed one).
///   2. The removed client's `Drop` blocks in `child.kill()` + `child.wait()`.
///      Running that on the caller's thread is dangerous: `detach` is invoked from
///      the `Destroyed` window-event handler (the **UI thread**), and holding any
///      thread hostage to a dying child — let alone the routing lock — froze the
///      launcher and every other product's IPC. So the blocking teardown is moved
///      to a throwaway thread; the caller returns instantly.
///
/// The on-disconnect callers pass an already-dead child, so that thread's
/// `kill()`+`wait()` returns at once — a negligible cost for one uniform path.
///
/// `reason` is for the diagnostic log only (e.g. `"window-closed"` /
/// `"disconnect"`), so an interleaved attach/detach trace says who tore down what.
pub fn detach(program: &str, reason: &str) {
    let removed = {
        let mut g = match OOP.write() {
            Ok(g) => g,
            Err(_) => return,
        };
        g.remove(program)
        // write lock released here ↓
    };
    // Offload the blocking child teardown. `Oop` is `Send` (it lives in the
    // process-wide map shared across threads), so moving it out is sound.
    match removed {
        Some(oop) => {
            tracing::info!("split_broker::detach({program}) gen={} (reason={reason})", oop.gen);
            std::thread::spawn(move || drop(oop));
        }
        None => tracing::debug!("split_broker::detach({program}) no-op — nothing attached (reason={reason})"),
    }
}

/// Detach `program` **only if** the currently-attached backend is generation
/// `gen`. Returns `true` when it removed that exact generation (the live backend
/// genuinely went away), `false` when a different/newer gen — or none — is
/// attached.
///
/// This closes the open/stop race: after an intentional stop the entry is already
/// gone (window-close `detach`) or replaced by a freshly-spawned newer gen, yet
/// the OLD child's disconnect callback still fires (its `kill()`+`wait()` finally
/// returns). An unconditional `detach` there would rip the NEW child out of the
/// routing map and raise a false "backend down" overlay on a perfectly healthy
/// respawn. Gating on `gen` makes the stale disconnect a logged no-op. The
/// blocking child drop is offloaded, same as [`detach`].
pub fn detach_if_current(program: &str, gen: u64, reason: &str) -> bool {
    let removed = {
        let mut g = match OOP.write() {
            Ok(g) => g,
            Err(_) => return false,
        };
        match g.get(program) {
            Some(o) if o.gen == gen => g.remove(program),
            _ => None,
        }
        // write lock released here ↓
    };
    match removed {
        Some(oop) => {
            tracing::info!("split_broker::detach_if_current({program}) gen={gen} removed (reason={reason})");
            std::thread::spawn(move || drop(oop));
            true
        }
        None => {
            tracing::warn!(
                "split_broker::detach_if_current({program}) gen={gen} IGNORED — stale (a newer gen or none is attached) (reason={reason})"
            );
            false
        }
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

/// Broker registered once per program. Holds the program label + an OPTIONAL
/// in-process loopback; the out-of-process channel lives in the shared [`OOP`] map
/// so it can be attached/detached over the backend's lifetime without rebuilding
/// the router.
///
/// `loopback`:
///   * `Some` — hybrid product (`corvus`): unrouted methods fall to the in-process
///     backend (which serves them, or itself reports `UnknownMethod`).
///   * `None` — pure-out-of-process product (`merula` / `sitta`): no fallback;
///     unrouted methods surface [`IpcError::BackendNotRunning`] /
///     [`IpcError::UnknownMethod`] depending on whether the backend is attached.
pub struct SplitBroker {
    program: &'static str,
    loopback: Option<Arc<dyn BrokerClient>>,
}

/// What to do with a call, decided under the read lock and acted on after it's
/// released (so a concurrent `detach()` on backend death never waits on an
/// in-flight round-trip).
enum Route {
    /// The attached backend advertises this method — send it there.
    Child(Arc<dyn BrokerClient>),
    /// A backend is attached but doesn't advertise this method.
    AttachedNoMethod,
    /// No backend is attached for this program.
    NotAttached,
}

impl SplitBroker {
    /// Hybrid product: unrouted methods fall to the in-process `loopback`.
    pub fn new(program: &'static str, loopback: Arc<dyn BrokerClient>) -> Self {
        Self { program, loopback: Some(loopback) }
    }

    /// Pure-out-of-process product: no in-process handlers, so no fallback. The
    /// three failure modes (unknown product / backend down / unknown method) stay
    /// distinct instead of collapsing into one catch-all sink.
    pub fn pure_oop(program: &'static str) -> Self {
        Self { program, loopback: None }
    }
}

impl BrokerClient for SplitBroker {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
        let route = match OOP.read() {
            Ok(g) => match g.get(self.program) {
                Some(o) if o.methods.contains(method) => Route::Child(o.child.clone()),
                Some(_) => Route::AttachedNoMethod,
                None => Route::NotAttached,
            },
            Err(_) => Route::NotAttached,
        };
        match route {
            Route::Child(c) => c.call(method, params),
            // Backend up but didn't advertise the method: a hybrid product can
            // still serve it in-process; a pure-OOP one genuinely doesn't have it.
            Route::AttachedNoMethod => match &self.loopback {
                Some(lb) => lb.call(method, params),
                None => Err(IpcError::UnknownMethod(method.to_string())),
            },
            // No backend attached: a hybrid product falls back in-process (the
            // shell still owns these handlers); a pure-OOP one reports that its
            // process isn't running — the method isn't "unknown", there's just
            // nothing to serve it.
            Route::NotAttached => match &self.loopback {
                Some(lb) => lb.call(method, params),
                None => Err(IpcError::BackendNotRunning(self.program.to_string())),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_ipc::prelude::LoopbackBroker;

    /// Echo backend advertising exactly the methods it's given — stands in for an
    /// attached child. Unique `program` labels per test keep the process-global
    /// `OOP` map collision-free across the suite.
    fn echo() -> Arc<dyn BrokerClient> {
        Arc::new(LoopbackBroker::new(|_m, p| Ok(p)))
    }

    #[test]
    fn pure_oop_reports_backend_not_running_when_detached() {
        let b = SplitBroker::pure_oop("sb-test-detached");
        let err = b.call("anything", Vec::new()).unwrap_err();
        assert!(matches!(err, IpcError::BackendNotRunning(p) if p == "sb-test-detached"));
    }

    #[test]
    fn pure_oop_routes_advertised_method_to_child() {
        attach("sb-test-attached", next_gen(), ["served"].iter().map(|s| s.to_string()).collect(), echo());
        let b = SplitBroker::pure_oop("sb-test-attached");
        assert_eq!(b.call("served", b"x".to_vec()).expect("routed"), b"x".to_vec());
        // Attached but unadvertised → UnknownMethod (NOT BackendNotRunning).
        let err = b.call("absent", Vec::new()).unwrap_err();
        assert!(matches!(err, IpcError::UnknownMethod(m) if m == "absent"));
        detach("sb-test-attached", "test");
    }

    #[test]
    fn hybrid_falls_back_to_loopback_when_detached() {
        // A hybrid product keeps serving in-process while its backend is down —
        // no BackendNotRunning, the loopback answers.
        let b = SplitBroker::new("sb-test-hybrid", echo());
        assert_eq!(b.call("whatever", b"y".to_vec()).expect("loopback"), b"y".to_vec());
    }
}
