//! [`IpcError`] — failures on the shell↔backend transport.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IpcError {
    /// The backend doesn't expose a command with this name.
    #[error("unknown method: {0}")]
    UnknownMethod(String),

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
