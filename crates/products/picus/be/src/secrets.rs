//! [`HostSecrets`] — resolving a connection's password without ever storing one.
//!
//! Picus keeps no password. The value lives in Arbor's keychain, shell-side, and is
//! fetched over the reverse channel at the moment a session is opened. This is the
//! only place in the backend that knows the mechanism; the driver crates see the
//! [`SecretResolver`] trait and nothing else.
//!
//! The `host_call` is synchronous (it parks on the reply channel), which is safe
//! here for the reason `docs/reverse-channel.md` spells out: the serve loop
//! dispatches each request onto its own worker thread, so blocking one waiting for
//! the shell never stalls the reader that has to deliver the answer.

use std::sync::Arc;

use arbor_ipc::prelude::HostCaller;
use picus_db_api::prelude::{DbError, DbResult, Secret, SecretResolver};

/// The host method the shell answers with a connection's stored secret.
///
/// The shell namespaces the keychain account itself (`picus/<id>`), so this call
/// cannot be used to read a git token by asking for someone else's account key.
const HOST_SECRET: &str = "__picus_secret";

/// Resolves secrets through the shell's credential broker.
pub struct HostSecrets {
    host: Option<Arc<dyn HostCaller>>,
}

impl HostSecrets {
    pub fn new(host: Option<Arc<dyn HostCaller>>) -> Self {
        Self { host }
    }
}

impl SecretResolver for HostSecrets {
    fn secret(&self, connection_id: &str) -> DbResult<Option<Secret>> {
        // No reverse channel means no keychain — every connection is then
        // password-less, which is correct for the in-process construction path and
        // honest everywhere else.
        let Some(host) = &self.host else { return Ok(None) };

        let value = host
            .call(HOST_SECRET, serde_json::json!(connection_id))
            .map_err(|e| DbError::Internal(format!("credential broker: {e}")))?;

        Ok(value.as_str().filter(|s| !s.is_empty()).map(|s| Secret::new(s.to_string())))
    }
}
