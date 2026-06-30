//! [`IpcError`] — failures on the shell↔backend transport.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IpcError {
    /// The backend doesn't expose a command with this name. Raised only when the
    /// product's backend IS running but never advertised the method (a typo or a
    /// not-yet-implemented command) — distinct from [`Self::BackendNotRunning`].
    #[error("unknown method: {0}")]
    UnknownMethod(String),

    /// The product's out-of-process backend isn't running (never spawned, or it
    /// died). The method may well exist — there's simply no process to serve it.
    /// Carries the product label. Raised by the shell's split router for a
    /// pure-out-of-process product with no attached backend; it never crosses the
    /// wire (a running backend can't report itself absent).
    #[error("backend not running: {0}")]
    BackendNotRunning(String),

    /// The transport itself failed (connection lost, handshake rejected, …).
    #[error("transport: {0}")]
    Transport(String),

    /// Serialising / deserialising a request or response failed.
    #[error("codec: {0}")]
    Codec(String),

    /// A backend-side error surfaced across the wire (already message-formatted).
    #[error("{0}")]
    Backend(String),
}

pub type Result<T> = std::result::Result<T, IpcError>;
