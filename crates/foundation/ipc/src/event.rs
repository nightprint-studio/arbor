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
