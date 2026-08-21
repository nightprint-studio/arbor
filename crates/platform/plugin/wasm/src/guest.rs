//! What a host function does, in the order it must do it.
//!
//! Deliberately **not** behind the `runtime` feature and deliberately free of wasmtime. The
//! interesting half of a host function is not that it crosses a component boundary — it is
//! that it asks the gate before it performs the effect, and that ordering is a property worth
//! testing without a `.wasm` to run it against.
//!
//! Every function here is the same three steps:
//!
//!   1. ask [`crate::caps::GuestCaps`] — the gate, which owns the rule *and* the guest's
//!      identity, so the guest cannot name another package;
//!   2. call [`crate::services::HostServices`] — the effect, which never re-checks because it
//!      never has to;
//!   3. map the outcome.
//!
//! A host function that performed the effect and then checked would be a host function that
//! had already done the thing.

use crate::caps::GuestCaps;
use crate::services::{HostRequest, HostResponse, Services};

/// Per-instance state every host function sees.
///
/// Holds the gate and the effects, and nothing else — notably not a plugin *name* separate
/// from the one inside [`GuestCaps`], because two of those is one that can drift.
pub struct GuestState {
    pub caps: GuestCaps,
    pub services: Services,
    /// WASI, present and granting nothing.
    ///
    /// Not a capability this crate wanted to hand out. `wasm32-wasip2` links the WASI standard
    /// library into every guest whether it uses it or not, so a component built for that
    /// target imports `wasi:io/poll` before running a line of its own code — and refusing to
    /// link WASI does not produce a guest without it, it produces a guest that will not
    /// instantiate.
    ///
    /// So the context is built empty: no preopened directories, no sockets, no inherited
    /// environment, no stdio. A guest still cannot open a file or a connection; the guarantee
    /// moved from what the linker omits to what the context contains, which is the same
    /// guarantee and a more honest description of it.
    #[cfg(feature = "runtime")]
    pub wasi: WasiState,
}

/// The WASI half of a guest's store data.
#[cfg(feature = "runtime")]
pub struct WasiState {
    pub table: wasmtime::component::ResourceTable,
    pub ctx: wasmtime_wasi::WasiCtx,
}

#[cfg(feature = "runtime")]
impl Default for WasiState {
    fn default() -> Self {
        Self {
            table: wasmtime::component::ResourceTable::new(),
            // Nothing is added to the builder. Every `inherit_*` and `preopened_dir` call is a
            // capability, and none is wanted.
            ctx: wasmtime_wasi::WasiCtxBuilder::new().build(),
        }
    }
}

#[cfg(feature = "runtime")]
impl wasmtime_wasi::WasiView for GuestState {
    fn table(&mut self) -> &mut wasmtime::component::ResourceTable {
        &mut self.wasi.table
    }
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.wasi.ctx
    }
}

impl GuestState {
    pub fn new(caps: GuestCaps, services: Services) -> Self {
        Self {
            caps,
            services,
            #[cfg(feature = "runtime")]
            wasi: WasiState::default(),
        }
    }

    /// Perform a request, gate first.
    pub fn fetch(&self, req: HostRequest) -> Result<HostResponse, String> {
        self.caps.allow_url(&req.url).map_err(|e| e.to_string())?;
        self.services.fetch(req)
    }

    /// Read one of the guest's own credentials, gate first.
    pub fn credential_get(&self, key: &str) -> Result<Option<String>, String> {
        let account = self.caps.credential_account(key).map_err(|e| e.to_string())?;
        self.services.credential_get(&account)
    }

    pub fn credential_set(&self, key: &str, value: &str) -> Result<(), String> {
        let account = self.caps.credential_account(key).map_err(|e| e.to_string())?;
        self.services.credential_set(&account, value)
    }

    pub fn credential_delete(&self, key: &str) -> Result<(), String> {
        let account = self.caps.credential_account(key).map_err(|e| e.to_string())?;
        self.services.credential_delete(&account)
    }

    /// Append to the package's log stream. The one capability with no gate, because there is
    /// nothing to gate: a guest talking into its own log cannot reach anything.
    pub fn log(&self, level: &str, message: &str) {
        self.services.log(self.caps.plugin(), level, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::HostServices;
    use std::sync::{Arc, Mutex};

    /// Services that record what they were asked to do, so a test can assert on whether the
    /// effect happened at all — which is the thing the ordering is about.
    #[derive(Default)]
    struct Spy {
        fetched: Mutex<Vec<String>>,
        reads:   Mutex<Vec<String>>,
        writes:  Mutex<Vec<(String, String)>>,
    }

    impl HostServices for Spy {
        fn credential_get(&self, account: &str) -> Result<Option<String>, String> {
            self.reads.lock().unwrap().push(account.to_string());
            Ok(Some("token".into()))
        }
        fn credential_set(&self, account: &str, value: &str) -> Result<(), String> {
            self.writes.lock().unwrap().push((account.to_string(), value.to_string()));
            Ok(())
        }
        fn credential_delete(&self, account: &str) -> Result<(), String> {
            self.writes.lock().unwrap().push((account.to_string(), String::new()));
            Ok(())
        }
        fn fetch(&self, req: HostRequest) -> Result<HostResponse, String> {
            self.fetched.lock().unwrap().push(req.url.clone());
            Ok(HostResponse { status: 200, headers: vec![], body: vec![] })
        }
        fn log(&self, _plugin: &str, _level: &str, _message: &str) {}
    }

    fn state() -> (GuestState, Arc<Spy>) {
        let spy = Arc::new(Spy::default());
        let caps = GuestCaps::new(
            "cloud-gcs",
            vec!["storage.googleapis.com".into()],
            vec!["oauth".into()],
        );
        (GuestState::new(caps, spy.clone()), spy)
    }

    fn req(url: &str) -> HostRequest {
        HostRequest {
            method: "GET".into(),
            url: url.into(),
            headers: vec![],
            body: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn an_allowed_request_reaches_the_service() {
        let (st, spy) = state();
        assert_eq!(st.fetch(req("https://storage.googleapis.com/b/o")).unwrap().status, 200);
        assert_eq!(spy.fetched.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_refused_request_never_reaches_the_service() {
        // The property, not the message: a denied URL must not have been sent. A gate that
        // ran after the effect would still return an error and would already have leaked the
        // request.
        let (st, spy) = state();
        assert!(st.fetch(req("https://evil.com/x")).is_err());
        assert!(spy.fetched.lock().unwrap().is_empty(), "the request was sent anyway");
    }

    #[test]
    fn a_credential_read_reaches_the_service_under_the_namespaced_account() {
        let (st, spy) = state();
        assert_eq!(st.credential_get("oauth").unwrap().as_deref(), Some("token"));
        assert_eq!(spy.reads.lock().unwrap()[0], "plugin/cloud-gcs/oauth");
    }

    #[test]
    fn an_undeclared_credential_never_reaches_the_service() {
        let (st, spy) = state();
        assert!(st.credential_get("sneaky").is_err());
        assert!(st.credential_set("sneaky", "v").is_err());
        assert!(st.credential_delete("sneaky").is_err());
        assert!(spy.reads.lock().unwrap().is_empty());
        assert!(spy.writes.lock().unwrap().is_empty(), "a write happened for an undeclared slot");
    }

    #[test]
    fn a_key_shaped_like_a_path_never_reaches_the_service() {
        // Even if it were somehow declared, the shape rule runs first — so nothing that could
        // resolve to one of Arbor's own accounts gets as far as the store.
        let st = GuestState::new(
            GuestCaps::new("x", vec![], vec!["../../github.com/arbor".into()]),
            Arc::new(Spy::default()),
        );
        assert!(st.credential_get("../../github.com/arbor").is_err());
    }
}
