//! The effects a wasm guest's host calls turn into.
//!
//! `arbor-plugin-wasm` decides **whether** a guest may do a thing — the network allowlist, the
//! credential namespace, the order in which the gate runs. This is the other half: the code
//! that actually reaches a keychain, a network and a log buffer, which is the shell's job and
//! nobody else's.
//!
//! Every method here is called **after** the gate has run, with an argument the gate produced:
//! an account name the namespace built, a URL whose host passed the allowlist. Nothing here
//! re-checks, and nothing here should need to — a second copy of a permission rule is a second
//! place for it to be wrong.
//!
//! ## Blocking, deliberately
//!
//! Guests are synchronous by contract (`wit/README.md`), which is what lets Arbor skip the
//! least-settled corner of the component model. The consequence lands here: [`fetch`] blocks
//! its thread while the host's runtime drives the request.
//!
//! That is only safe because a guest is invoked from `spawn_blocking` — the landmine the whole
//! backend is built around (`docs/backend-architecture.md`). Calling this from a runtime worker
//! would occupy the worker that has to complete the future it is waiting on, which is the
//! deadlock that produces white windows.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use arbor_plugin_wasm::prelude::{
    ExtensionIndex, GuestCaps, HostRequest, HostResponse, HostServices, Services, WasmHost,
};

pub struct TauriHostServices {
    /// Where a guest's log lines go. A closure rather than an `AppHandle`, because a platform
    /// handler holds only `&AppState` — and the one thing this needs a handle FOR is the
    /// Plugin Logs ring buffer, which is a function call away from anything that has one.
    log_line: Box<dyn Fn(&str, &str, &str) + Send + Sync>,
    http:     reqwest::Client,
}

impl TauriHostServices {
    /// Build services with somewhere for a guest's log lines to go.
    ///
    /// A closure rather than an `AppHandle` because a platform handler holds only
    /// `&AppState`. The Plugin Logs variant is one line —
    /// `crate::plugin_logs::record(&handle, level, plugin, msg.to_string())` — and belongs at
    /// the first call site that has a handle and a guest worth logging, not here waiting for
    /// one.
    pub fn new(log_line: Box<dyn Fn(&str, &str, &str) + Send + Sync>) -> Self {
        // One client, so connection pooling and DNS caching survive across calls — a guest
        // listing a bucket makes a request per page, and a fresh client per call would
        // renegotiate TLS every time.
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .unwrap_or_default();
        Self { log_line, http }
    }
}

impl HostServices for TauriHostServices {
    fn credential_get(&self, account: &str) -> Result<Option<String>, String> {
        crate::auth::credential_store::get(account, "").map_err(|e| e.to_string())
    }

    fn credential_set(&self, account: &str, value: &str) -> Result<(), String> {
        crate::auth::credential_store::save(account, "", value).map_err(|e| e.to_string())
    }

    fn credential_delete(&self, account: &str) -> Result<(), String> {
        crate::auth::credential_store::delete(account, "").map_err(|e| e.to_string())
    }

    fn fetch(&self, req: HostRequest) -> Result<HostResponse, String> {
        let client = self.http.clone();
        // `block_on` on the host runtime. Safe here and nowhere else: see the module note.
        tauri::async_runtime::block_on(async move {
            let method = reqwest::Method::from_bytes(req.method.as_bytes())
                .map_err(|_| format!("'{}' is not an HTTP method", req.method))?;
            let mut builder = client.request(method, &req.url);
            for (name, value) in &req.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            if let Some(secs) = req.timeout_secs {
                // Clamped: a guest cannot hold a blocking thread for as long as it likes, and
                // a two-minute ceiling is already generous for an object-storage call.
                builder = builder.timeout(Duration::from_secs(secs.clamp(1, 120) as u64));
            }
            if let Some(body) = req.body {
                builder = builder.body(body);
            }
            let res = builder.send().await.map_err(|e| e.to_string())?;
            let status = res.status().as_u16();
            let headers = res
                .headers()
                .iter()
                .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
                .collect();
            // A 4xx or 5xx is a response, not an error: the request succeeded and the server
            // said no. Collapsing the two would leave a guest unable to tell a bucket that
            // returned 404 from a network that is down.
            let body = res.bytes().await.map_err(|e| e.to_string())?.to_vec();
            Ok(HostResponse { status, headers, body })
        })
    }

    fn log(&self, plugin: &str, level: &str, message: &str) {
        (self.log_line)(plugin, level, message);
    }
}

// ── The process's one engine ────────────────────────────────────────────────────

/// One `WasmHost` for the process, so compiled modules are shared.
///
/// A compile is the expensive part and a module is immutable once installed, so a second
/// engine would mean compiling everything twice for no isolation gain — isolation is per
/// `Store`, and every guest gets its own.
static ENGINE: OnceLock<Result<WasmHost, String>> = OnceLock::new();

/// The engine, for host code that drives a guest directly (see `crate::cloud_guest`).
pub fn engine() -> Result<&'static WasmHost, String> {
    ENGINE
        .get_or_init(|| WasmHost::new().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| e.clone())
}

/// Drop every cached component compilation.
///
/// Called whenever the plugin set moves — a marketplace install, and every plugin reload.
/// Without it a reinstall writes a different module to the same path and the engine keeps
/// handing back the previous build: the user updates a package, nothing changes, and nothing
/// anywhere says why. The cache is a speed optimisation and this is what keeps it from being a
/// correctness one.
///
/// Reload matters as much as install, and for the same reason one directory further out: a
/// package developed in place is rebuilt by its own `build.sh`, never by Arbor, so Reload is
/// the ONLY moment the app is told the bytes on disk are not the bytes it compiled.
pub fn forget_compiled() {
    // Only if an engine was ever built — creating one here just to clear it would compile
    // Cranelift's machinery on a path that has nothing to do.
    if let Some(Ok(host)) = ENGINE.get() {
        host.forget_all();
    }
}

/// Bring one extension up and let it go again.
///
/// The point is the bringing up. Instantiating exercises the whole chain — the module
/// compiles, its imports resolve against what this host offers, its exports match the world
/// it claims — and every one of those failures is otherwise invisible until the first time
/// somebody opens a file and nothing happens.
///
/// **Blocking**, and deliberately: a guest is synchronous and a host call it makes on the way
/// up blocks with it. Safe from a platform handler because those already run on the blocking
/// pool — see `crate::ipc::platform::fs`'s note — and unsafe from anywhere that does not.
pub fn probe(interface: &str, version: u32, id: &str) -> Result<(), String> {
    let manifests = arbor_plugin_core::prelude::discover_plugins()
        .map_err(|e| e.to_string())?;
    let enabled = arbor_plugin_core::prelude::load_plugin_states();
    let index = ExtensionIndex::build(&manifests, &enabled);

    let entry = index
        .resolve(interface, version, id)
        .ok_or_else(|| format!("no package provides {interface}@{version}/{id}"))?;
    let manifest = manifests
        .iter()
        .find(|m| m.name == entry.plugin)
        // Cannot happen — the index was built from these — but an unwrap here would be an
        // unwrap in a command a user can invoke.
        .ok_or_else(|| format!("'{}' vanished between discovery and probe", entry.plugin))?;

    let caps = GuestCaps::from_manifest(manifest);
    // A guest that logs during instantiation has nothing to say yet, and a probe is not where
    // a log line earns its place — it goes to tracing, and the Plugin Logs wiring
    // (`with_handle`) is for the real call sites.
    let plugin_name = entry.plugin.clone();
    let services: Services = Arc::new(TauriHostServices::new(Box::new(
        move |_plugin: &str, level: &str, message: &str| {
            tracing::debug!("[{plugin_name}] probe {level}: {message}");
        },
    )));
    let host = engine()?;

    // Instantiated through the DYNAMIC path, not a match on the interface name.
    //
    // The typed openers know two worlds, and a probe written against them answered "not an
    // interface this host knows how to instantiate" for every other one — which is a refusal
    // to look, reported as if the package were broken. The whole point of the extension seam
    // is that the host does not know what an interface is, so neither does its probe.
    let guest = host
        .open_dynamic(&entry.module, caps, services)
        .map_err(|e| e.to_string())?;

    // Coming up is necessary and not sufficient: a package can instantiate and export nothing
    // its manifest claimed. Checked here because this is the one place that looks, and a row
    // reading "runs" for a module with no matching export would be worse than no row.
    let exports = guest.surface(host.engine());
    let wanted = format!("/{interface}@");
    if exports.iter().any(|e| e.name.contains(&wanted) || e.name.ends_with(&format!("/{interface}"))) {
        return Ok(());
    }
    if exports.is_empty() {
        return Err("this module exports no interface at all".to_string());
    }
    Err(format!(
        "declares {interface} but exports {}",
        exports.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
    ))
}
