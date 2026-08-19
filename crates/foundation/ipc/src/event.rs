//! Push events — the one-way BE→shell channel (LSP-notification style).
//!
//! Events never ride the request/response channel. They flow on a dedicated
//! one-way channel as length-prefixed serde messages, where the shell applies
//! throttling / coalescing / backpressure before re-emitting to the FE as Tauri
//! events. See `docs/ipc-design.md`.

use serde::{Deserialize, Serialize};

/// A single backend→shell push event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// A routable notification: the shell re-emits it to the FE as the Tauri
    /// event `topic` carrying `payload` (the existing emit/listen mechanism).
    Notify {
        topic:   String,
        payload: serde_json::Value,
    },
    /// Liveness heartbeat (drives the auto-reconnect bookkeeping, CLAUDE.md).
    Ping,
}

/// Egress for backend → frontend events — the producer side of [`Event`].
///
/// A handler reached through the IPC seam holds only its backend state, never an
/// `AppHandle`; it pushes events through an `EventSink`. In-process the shell
/// backs it with `AppHandle::emit`; once a backend splits into its own process
/// the sink wraps the [`Event`] channel and each `emit` becomes an
/// `Event::Notify { topic, payload }` the shell re-emits to the FE. **The call
/// site never changes — only the backing.**
///
/// Object-safe (one method, `serde_json::Value` payload) on purpose: backend
/// state holds an `Arc<dyn EventSink>` that clones cheaply into background
/// threads/tasks which outlive a call and emit from inside.
pub trait EventSink: Send + Sync {
    /// Emit the frontend event `topic` carrying `payload`.
    fn emit(&self, topic: &str, payload: serde_json::Value);

    /// Narrate a long piece of work while it is still going.
    ///
    /// A handler that takes minutes — a test run, a build — used to have exactly two
    /// things to say: the id it started with, and the answer, separated by silence. The
    /// panel filled in from the domain's own events, but nothing else could: an AI client
    /// waiting on the call has no listener, and a caller that cannot tell "working" from
    /// "hung" eventually stops waiting.
    ///
    /// So this is the one topic that means *what is happening now*, in words, for anyone
    /// who is waiting rather than rendering. Domain events stay as they are — this does not
    /// replace them, and a panel should keep using the ones shaped for it.
    ///
    /// `done` / `total` are for the callers that draw a bar; either may be absent, because
    /// most long work does not know its own length until it ends.
    fn progress(&self, message: &str, done: Option<u64>, total: Option<u64>) {
        let mut payload = serde_json::json!({ "message": message });
        if let Some(done) = done {
            payload["done"] = serde_json::json!(done);
        }
        if let Some(total) = total {
            payload["total"] = serde_json::json!(total);
        }
        self.emit(PROGRESS_TOPIC, payload);
    }
}

/// The topic [`EventSink::progress`] emits on.
///
/// One topic for every backend, on purpose: the shell forwards it to whoever is waiting on
/// the call that produced it, and it can only do that if it does not have to know which
/// product's vocabulary this particular run speaks.
pub const PROGRESS_TOPIC: &str = "arbor://progress";
