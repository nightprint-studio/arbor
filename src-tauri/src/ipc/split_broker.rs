//! `SplitBroker` — routes each `corvus` method to the right place during the
//! in-process → out-of-process migration.
//!
//! A method the spawned `corvus-be` advertised (in its `Hello`) goes to the
//! child process; everything else stays in-process on the `LoopbackBroker`.
//! Moving a handler into `corvus-be` therefore flips its routing automatically —
//! no per-method edits here. See `docs/corvus-be-bringup.md`.

use std::collections::HashSet;
use std::sync::Arc;

use arbor_ipc::prelude::{BrokerClient, Bytes, IpcError};

pub struct SplitBroker {
    /// Method names served out-of-process by `corvus-be`.
    oop_methods: HashSet<String>,
    child: Arc<dyn BrokerClient>,
    loopback: Arc<dyn BrokerClient>,
}

impl SplitBroker {
    pub fn new(
        oop_methods: HashSet<String>,
        child: Arc<dyn BrokerClient>,
        loopback: Arc<dyn BrokerClient>,
    ) -> Self {
        Self { oop_methods, child, loopback }
    }
}

impl BrokerClient for SplitBroker {
    fn call(&self, method: &str, params: Bytes) -> Result<Bytes, IpcError> {
        if self.oop_methods.contains(method) {
            self.child.call(method, params)
        } else {
            self.loopback.call(method, params)
        }
    }
}
