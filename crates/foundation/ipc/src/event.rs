//! Push events — the one-way BE→shell channel (LSP-notification style).
//!
//! `tarpc` doesn't stream by design, so events never ride the request/response
//! channel. They flow on a dedicated one-way channel as length-prefixed serde
//! messages, where the shell applies throttling / coalescing / backpressure
//! before re-emitting to the FE as Tauri events. See `docs/ipc-design.md`.

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
}
