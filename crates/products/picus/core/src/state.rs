//! [`PicusState`] — the headless picus backend's owned state.
//!
//! Mirrors `bennu-core`'s `BennuState`: transport-only. A SQL studio's heavy state
//! (the live driver sessions, the parsed script inventory, the per-project model)
//! belongs to the crates the domain handlers own — the connection registry lands
//! with `picus-db-api`, the inventory with `picus-inventory` — and each gains a
//! `with_*` builder here rather than a new constructor, so a later wave never has
//! to re-edit this file.
//!
//! NOTE: no field here is, or will be, an "active dialect". The dialect travels
//! with the folder being written (`docs/picus-design.md` §1); a backend-wide
//! current dialect would quietly break the product's single reason to exist.

use std::sync::Arc;

use arbor_ipc::prelude::{EventSink, HostCaller};
use picus_db_api::prelude::DbProviderRegistry;
use serde_json::Value;

use crate::connections::SessionPool;

/// The state every picus-be handler gets, `Arc`-shared across the dispatcher and
/// any background workers (a long query, an inventory index build).
pub struct PicusState {
    /// Backend → frontend event egress. The shell re-emits each topic to the Picus
    /// window. Call sites use [`emit`](Self::emit) / [`event_sink`](Self::event_sink).
    sink: Arc<dyn EventSink>,
    /// Reverse channel back to the shell (`docs/reverse-channel.md`), set from the
    /// `App`'s host caller. Load-bearing for Picus specifically: **the product
    /// stores no password**, so a connection's secret is resolved through the
    /// shell's credential broker at the moment of use, over this channel. `None`
    /// only in the (unused) in-process construction path.
    host: Option<Arc<dyn HostCaller>>,
    /// The engines this backend can open a session to. Populated at boot by
    /// `picus-be`, which is the only place that knows which driver crates are
    /// linked. An engine absent from here is still fully supported on the *script*
    /// side — that is the Oracle case today.
    providers: DbProviderRegistry,
    /// The sessions currently open. In memory only: a database session's lifetime
    /// is the backend process, never the disk.
    sessions: SessionPool,
}

impl PicusState {
    /// Build the backend state from its event egress. Wave-friendly: a new piece
    /// gains a `with_*` builder rather than a new constructor.
    pub fn new(sink: Arc<dyn EventSink>) -> Self {
        Self {
            sink,
            host: None,
            providers: DbProviderRegistry::new(),
            sessions: SessionPool::new(),
        }
    }

    /// Attach the reverse channel back to the shell (the `App`'s host caller).
    pub fn with_host_caller(mut self, host: Arc<dyn HostCaller>) -> Self {
        self.host = Some(host);
        self
    }

    /// Attach the registry of database engines this backend links.
    pub fn with_providers(mut self, providers: DbProviderRegistry) -> Self {
        self.providers = providers;
        self
    }

    /// The registered database engines.
    pub fn providers(&self) -> &DbProviderRegistry {
        &self.providers
    }

    /// The live-session pool.
    pub fn sessions(&self) -> &SessionPool {
        &self.sessions
    }

    /// Emit a frontend event. The shell re-emits the topic to the Picus window.
    pub fn emit(&self, topic: &str, payload: Value) {
        self.sink.emit(topic, payload);
    }

    /// A cloneable handle to the event egress, for a background worker (a running
    /// query streaming progress) that emits from inside and outlives the borrow of
    /// `&self`.
    pub fn event_sink(&self) -> Arc<dyn EventSink> {
        Arc::clone(&self.sink)
    }

    /// Call back into the shell, blocking on the reply. Errors with a clear message
    /// when no reverse channel is wired.
    pub fn host_call(&self, method: &str, params: Value) -> Result<Value, String> {
        match &self.host {
            Some(h) => h.call(method, params),
            None => Err(format!("host_call('{method}'): no reverse channel (in-process)")),
        }
    }

    /// A cloneable handle to the reverse channel, for a background worker (the
    /// credential resolution a pooled reconnect needs).
    pub fn host_caller(&self) -> Option<Arc<dyn HostCaller>> {
        self.host.clone()
    }
}
