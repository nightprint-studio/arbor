//! [`HostCaller`] — the reentrant backend→shell request/response channel
//! (`docs/reverse-channel.md`).
//!
//! The twin of [`EventSink`](crate::event::EventSink): where `EventSink::emit`
//! is one-way, fire-and-forget egress, `HostCaller::call` lets a backend
//! **originate a request to the shell and block on the reply** — the missing
//! transport direction an OOP backend needs to resolve a credential (the keyring
//! lives shell-side) or wait on a plugin-UI round-trip.
//!
//! Object-safe (one method, JSON payload) on purpose: backend state holds an
//! `Arc<dyn HostCaller>` and the call site never changes between in-process and
//! split-out — only the backing does. In-process a backend reaches the shell
//! directly and never needs this; once it splits into its own process the
//! backing becomes [`FrameHostCaller`](crate::transport::FrameHostCaller), which
//! marshals the call as a `HostRequest` frame and awaits the `HostResponse`.

use serde_json::Value;

/// Reentrant backend→shell request/response. `call` blocks until the shell
/// answers; `Err` carries the shell-side error as a wire string (mirroring the
/// forward channel's `Response` error mapping).
pub trait HostCaller: Send + Sync {
    /// Invoke shell host-method `method` with `params`, blocking on the reply.
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}
