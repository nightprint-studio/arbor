//! What the embedder lends a guest.
//!
//! The three capabilities in `wit/host.wit` — secrets, http, log — have to be *performed* by
//! something that can reach a keychain, a network and a log buffer. None of those belong in
//! this crate: it decides **whether** a guest may do a thing, and the shell does it.
//!
//! So the embedder passes a [`HostServices`] once, and the engine calls through it. That
//! keeps the gate and the effect on opposite sides of a boundary — this crate can be tested
//! with services that do nothing, and the shell's implementation never has to reason about
//! permissions because it is only ever called after they were checked.
//!
//! ## Why these are synchronous
//!
//! Because guests are. A guest runs on a blocking worker and the host drives the async work
//! while that worker waits — the same shape as every other path into an Arbor backend. An
//! async signature here would push the component model's least-settled corner into an
//! interface that has no need of it.

use std::sync::Arc;

/// One HTTP request, as a guest asked for it.
#[derive(Debug, Clone)]
pub struct HostRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_secs: Option<u32>,
}

/// What came back.
#[derive(Debug, Clone)]
pub struct HostResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// The effects a guest's host calls turn into.
///
/// Every method receives an **already-resolved, already-approved** argument: an account name
/// the namespace produced, a URL whose host passed the allowlist. Implementations do not
/// re-check and must not need to.
pub trait HostServices: Send + Sync + 'static {
    /// Read a credential by its resolved account name.
    fn credential_get(&self, account: &str) -> Result<Option<String>, String>;
    fn credential_set(&self, account: &str, value: &str) -> Result<(), String>;
    fn credential_delete(&self, account: &str) -> Result<(), String>;

    /// Perform an approved request. Blocking: see the module note.
    fn fetch(&self, req: HostRequest) -> Result<HostResponse, String>;

    /// Append to the package's stream in the Plugin Logs panel.
    fn log(&self, plugin: &str, level: &str, message: &str);
}

/// Services that do nothing, for tests and for a host with none of this wired up.
///
/// Refuses rather than silently succeeding: a guest that "stored" a token into a void and got
/// `Ok` would fail much later and somewhere unrelated.
pub struct NoServices;

impl HostServices for NoServices {
    fn credential_get(&self, _account: &str) -> Result<Option<String>, String> {
        Err("no credential store on this host".into())
    }
    fn credential_set(&self, _account: &str, _value: &str) -> Result<(), String> {
        Err("no credential store on this host".into())
    }
    fn credential_delete(&self, _account: &str) -> Result<(), String> {
        Err("no credential store on this host".into())
    }
    fn fetch(&self, _req: HostRequest) -> Result<HostResponse, String> {
        Err("no network on this host".into())
    }
    fn log(&self, plugin: &str, level: &str, message: &str) {
        tracing::debug!("[{plugin}] {level}: {message}");
    }
}

/// Shared handle, since every instantiated guest holds one.
pub type Services = Arc<dyn HostServices>;
