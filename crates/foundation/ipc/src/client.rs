//! [`BrokerClient`] — the transport-agnostic request/response client the shell
//! router speaks to, plus the in-process [`LoopbackBroker`] used by M3's
//! in-process-first step (and, here in M1, by the ping round-trip test).
//!
//! The same trait fronts both transports: an in-memory loopback today and a
//! `tarpc`-over-named-pipe/unix-socket client tomorrow. Swapping the transport
//! is the "flip" of principle #6 — the router never changes.

use crate::error::{IpcError, Result};

/// A request/response payload on the wire. The loopback keeps it opaque; the
/// production transport carries a serde binary codec (bincode/postcard). JSON is
/// only used in dev loopback call sites.
pub type Bytes = Vec<u8>;

/// The request/response client the router uses, regardless of transport.
pub trait BrokerClient: Send + Sync {
    /// Invoke `method` on the backend with a parameter blob, returning the
    /// result blob. Errors map backend failures, unknown methods and transport
    /// faults onto [`IpcError`].
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes>;
}

/// An in-process backend: no serialization-over-a-socket, no second process.
/// Holds a single dispatch closure `(method, params) -> result`. This is what
/// M3 uses to move the 547 commands behind `arbor-ipc` while everything still
/// runs in one process; later the same call sites talk to a pipe-backed client.
pub struct LoopbackBroker {
    dispatch: Box<dyn Fn(&str, Bytes) -> Result<Bytes> + Send + Sync>,
}

impl LoopbackBroker {
    /// Build a loopback over a dispatch closure.
    pub fn new<F>(dispatch: F) -> Self
    where
        F: Fn(&str, Bytes) -> Result<Bytes> + Send + Sync + 'static,
    {
        Self { dispatch: Box::new(dispatch) }
    }

    /// A trivial loopback that answers `"ping"` by echoing its params and
    /// rejects everything else. Used by the M1 round-trip smoke test and as the
    /// minimal stand-in until real backends register their dispatch.
    pub fn ping_only() -> Self {
        Self::new(|method, params| match method {
            "ping" => Ok(params),
            other => Err(IpcError::UnknownMethod(other.to_string())),
        })
    }
}

impl BrokerClient for LoopbackBroker {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes> {
        (self.dispatch)(method, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_ping_round_trips() {
        let broker = LoopbackBroker::ping_only();
        let out = broker.call("ping", b"hello".to_vec()).expect("ping ok");
        assert_eq!(out, b"hello".to_vec());
    }

    #[test]
    fn loopback_rejects_unknown_method() {
        let broker = LoopbackBroker::ping_only();
        let err = broker.call("nope", Vec::new()).unwrap_err();
        assert!(matches!(err, IpcError::UnknownMethod(_)));
    }
}
