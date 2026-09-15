//! `bennu-dap` — a client for the **Debug Adapter Protocol**.
//!
//! What `bennu-jdwp` is for Java, this is for everything else: the transport a breakpoint debugger
//! needs, and nothing above it. No session policy, no UI, no opinion about what a breakpoint means
//! to a project.
//!
//! ## Why DAP is not a second JDWP
//!
//! JDWP is a wire protocol spoken by the thing being debugged — the JVM *is* the debugger's peer. DAP
//! is spoken by an **adapter**: a separate process that drives a real native debugger (LLDB, GDB) and
//! translates. So this crate does two things JDWP's client does not: it spawns and supervises a child
//! process, and it has to find one first, because nothing ships a debug adapter with the Rust
//! toolchain ([`discovery`]).
//!
//! It also means the adapter can ask *us* things mid-session — `runInTerminal`, `startDebugging` — and
//! is blocked until answered. A client that only expects responses and events hangs on the first one.
//! See [`protocol::Incoming::classify`].
//!
//! ## The modules
//!
//! * [`protocol`] — the envelope: request, response, event, and how an incoming message is
//!   classified. The `Content-Length` framing under it is [`bennu_framed`], shared with the LSP
//!   client because Microsoft specified the same envelope for both.
//! * [`types`] — the request arguments and response bodies Bennu uses, hand-rolled and bounded.
//! * [`discovery`] — which adapter can debug a Rust binary, and where it is on this machine.
//! * [`rendering`] — what it can *show* you: teaching a plain LLDB about Rust's types, and the
//!   expression dialects the adapters differ on.
//! * [`client`] — the process and the correlation. Moves messages, decides nothing.
//! * [`session`] — the handshake and the operations a debugger panel needs.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_dap::prelude::...`.

pub mod client;
pub mod discovery;
pub mod prelude;
pub mod protocol;
pub mod rendering;
pub mod session;
pub mod types;
