//! `BackendIo` — the framed-stdio plumbing every Model-D backend builds first.
//!
//! Four pieces, identical across `corvus-be` / `merula-be` / `sitta-be`:
//! - **`stdout`** — the single `SharedWriter` the serve loop and the event sink
//!   share (a mutex serialises the two onto the one protocol channel);
//! - **`sink`** — event egress (`FrameEventSink`), what the product state holds to
//!   emit backend→shell events;
//! - **`host`** — the reverse channel caller (`FrameHostCaller`), what handlers
//!   reach the shell through (and what the serve loop demuxes `HostResponse`s on);
//! - **`rt`** — the multi-thread tokio runtime async handlers `block_on`.
//!
//! `main` builds one and hands it to [`crate::App`]; the product reads `sink` /
//! `host_caller` / `runtime_handle` off it to wire its own state before the move.

use std::io;
use std::sync::{Arc, Mutex};

use arbor_ipc::prelude::{EventSink, FrameEventSink, FrameHostCaller, HostCaller, SharedWriter};

/// The framed-stdio egress + reverse channel + runtime a backend serves on.
pub struct BackendIo {
    /// The shared protocol writer — frames (responses) and events both go here.
    pub stdout: SharedWriter,
    /// Event egress; the product state holds a clone to emit backend→shell events.
    pub sink: Arc<dyn EventSink>,
    /// The reverse-channel caller (kept concrete — `serve_stdio` needs the
    /// `Arc<FrameHostCaller>`; use [`host_caller`](Self::host_caller) for the
    /// `dyn HostCaller` registries want).
    pub host: Arc<FrameHostCaller>,
    /// The multi-thread runtime async handlers `block_on`. Kept alive for the
    /// whole serve loop (handlers hold `Handle`s into it).
    pub rt: tokio::runtime::Runtime,
}

impl BackendIo {
    /// Build the four pieces over the process's real stdout.
    pub fn new() -> Self {
        // stdout carries frames; logs go to stderr.
        let stdout: SharedWriter = Arc::new(Mutex::new(io::stdout()));
        // Egress: a frame sink writing onto the same stdout the serve loop uses.
        let sink: Arc<dyn EventSink> = Arc::new(FrameEventSink::new(Arc::clone(&stdout)));
        // Reverse channel: writes `HostRequest` frames on the same stdout the
        // serve loop routes `HostResponse`s back through.
        let host = FrameHostCaller::new(Arc::clone(&stdout));
        // The serve loop dispatches each request on its own thread, so concurrent
        // `block_on`s land here — a multi-thread runtime is required.
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("arbor-be: failed to build tokio runtime");
        Self { stdout, sink, host, rt }
    }

    /// The reverse-channel caller as a trait object — for the product state and
    /// the credential-resolving registries.
    pub fn host_caller(&self) -> Arc<dyn HostCaller> {
        Arc::clone(&self.host) as Arc<dyn HostCaller>
    }

    /// A clone of the event sink (held by the product state / `AppCtx`).
    pub fn sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    /// A handle to the runtime (for the `AppCtx` and the async-dispatch
    /// `block_on`). The runtime itself stays in [`BackendIo`] / [`crate::App`].
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.rt.handle().clone()
    }
}

impl Default for BackendIo {
    fn default() -> Self {
        Self::new()
    }
}
