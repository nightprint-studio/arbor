//! `arbor-ipc` — the transport for Arbor's Model D (1 FE + N BE).
//!
//! Two channels between the shell process and each headless product backend,
//! LSP-style:
//!
//! - **Commands (request/response)**: a method name + JSON params, dispatched
//!   over a single [`prelude::BrokerClient`] — one command ≈ one router entry.
//! - **Events (push, BE → shell)** on a dedicated one-way channel: a single
//!   length-prefixed [`prelude::Event`], with throttling/coalescing/backpressure.
//!   Events never ride the request/response channel.
//!
//! Transport-agnostic via [`prelude::BrokerClient`]: the in-process
//! [`prelude::LoopbackBroker`] for same-process dispatch, and
//! [`prelude::ChildClient`] for an out-of-process backend over **framed JSON on
//! the child's stdin/stdout** (parent spawns child, reads its `Hello`, demuxes
//! responses / events / host-calls). The same router, handlers and frame
//! protocol stay put when the byte-stream is later hardened to a named pipe
//! (Windows) / unix socket (`0600` + `SO_PEERCRED`) + nonce/ACL — only the
//! listener/connector under [`prelude::ChildClient`] changes. Full design (frame
//! protocol, handshake, the flip table): `docs/ipc-design.md`.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `arbor_ipc::prelude::...`.

pub mod client;
pub mod credential;
pub mod error;
pub mod event;
pub mod host;
pub mod prelude;
pub mod stream;
pub mod transport;
