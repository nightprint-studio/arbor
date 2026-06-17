//! `arbor-ipc` — the transport for Arbor's Model D (1 FE + N BE).
//!
//! Two channels between the shell process and each headless product backend,
//! LSP-style:
//!
//! - **Commands (request/response)** via `tarpc`: a `#[tarpc::service]` trait
//!   generates the typed client + server, so one command ≈ one definition.
//! - **Events (push, BE → shell)** on a dedicated one-way channel: a single
//!   length-prefixed [`prelude::Event`], with throttling/coalescing/backpressure.
//!   `tarpc` does not stream by design, so events never ride the RPC channel.
//!
//! Transport-agnostic via [`prelude::BrokerClient`]: an in-process loopback
//! today, a named pipe (Windows) / unix socket (`0600` + `SO_PEERCRED`)
//! tomorrow, with a spawn parent→child + nonce handshake. The same client runs
//! on both by swapping only the transport.
//!
//! ## Scope of this milestone (M1b)
//!
//! This skeleton ships only the transport-agnostic contract
//! ([`prelude::BrokerClient`], [`prelude::Event`], [`prelude::IpcError`]) plus
//! the in-process [`prelude::LoopbackBroker`] with a ping round-trip. The
//! `tarpc` codegen, the named-pipe/unix-socket transport and the nonce/ACL
//! handshake land at the M3 in-process→IPC flip. Full design (service shape,
//! handshake, the flip table): `docs/ipc-design.md`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `arbor_ipc::prelude::...`.

pub mod client;
pub mod credential;
pub mod error;
pub mod event;
pub mod prelude;
pub mod transport;
