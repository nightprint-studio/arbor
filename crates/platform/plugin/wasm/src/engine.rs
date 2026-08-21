//! Instantiating a component — the wasmtime layer, and the only file that knows what a
//! `Store` is.
//!
//! Everything else in this crate is pure: [`crate::registry`] decides which package
//! implements what, [`crate::caps`] decides what a guest may reach, [`crate::guest`] holds
//! the gate-then-effect ordering, and [`crate::services`] is how the embedder performs the
//! effects. That is the load-bearing half and it is tested without a runtime.
//!
//! ## Feature-gated on `runtime`, off by default
//!
//! wasmtime is megabytes of dependency and these bindings are generated from `wit/` at
//! compile time. Bringing that up has its own iteration loop, and the half of this crate that
//! encodes Arbor's rules should not wait behind it — nor should the rest of the workspace.
//!
//! ## One engine, many guests
//!
//! [`WasmHost`] holds a single `Engine`, so compiled code is shared and the second document a
//! format backend opens costs an instantiation rather than a compile. Each *instance* gets
//! its own `Store`, which is what keeps one guest's memory and one guest's capabilities from
//! being another's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};

use crate::caps::GuestCaps;
use crate::guest::GuestState;
use crate::services::Services;

/// Why a guest could not be brought up.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("reading {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The guest ran and returned an error of its own — a bucket that does not exist, a token
    /// that was refused. Distinct from [`EngineError::Wasm`], which is the guest failing to
    /// run at all: one is the extension working correctly and saying no, the other is the
    /// extension being broken, and a caller that could not tell them apart would retry the
    /// wrong one.
    #[error("{0}")]
    Guest(String),

    /// Anything wasmtime refused. `wasmtime::Error` is the re-export of `anyhow::Error`, so
    /// this carries the engine's own message rather than a flattened string.
    #[error("{context}: {source}")]
    Wasm {
        context: String,
        #[source]
        source: wasmtime::Error,
    },
}

impl EngineError {
    /// Attach context to a wasmtime failure. `pub(crate)` because `dispatch` builds the same
    /// kind of error and there is no reason for two spellings of it.
    pub(crate) fn wasm(context: impl Into<String>) -> impl FnOnce(wasmtime::Error) -> Self {
        let context = context.into();
        move |source| EngineError::Wasm { context, source }
    }
}

/// The process-wide component host.
pub struct WasmHost {
    engine: Engine,
    /// Compiled components keyed by module path.
    ///
    /// A compile is expensive, so this is a memo — but **not** one that can be left to expire
    /// on its own. A reinstall writes a different module to the *same* path, so a cache keyed
    /// by path hands back the previous build and keeps doing it until the process restarts:
    /// the user updates a package, nothing changes, and nothing anywhere says why.
    ///
    /// So it is dropped explicitly, by [`WasmHost::forget_all`], whenever the plugin set moves.
    compiled: Mutex<HashMap<PathBuf, Component>>,
}

impl WasmHost {
    pub fn new() -> Result<Self, EngineError> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        // No `async_support(false)` call: that method only exists when wasmtime's `async`
        // feature is on, and it is not — synchronous IS this build. Guests are synchronous by
        // contract (`wit/README.md`), so there is nothing to turn off.
        // Fuel stays off. These are extensions the user installed and consented to, and a
        // budget that stops a legitimate parse halfway is worse than one that never stops a
        // runaway. Epoch interruption is the thing to reach for if that changes: it can
        // cancel without paying a per-instruction cost.
        let engine = Engine::new(&config).map_err(EngineError::wasm("engine"))?;
        Ok(Self { engine, compiled: Mutex::new(HashMap::new()) })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Compile a module, or hand back the cached compilation.
    pub fn component(&self, path: &Path) -> Result<Component, EngineError> {
        if let Ok(cache) = self.compiled.lock() {
            if let Some(c) = cache.get(path) {
                return Ok(c.clone());
            }
        }
        let bytes = std::fs::read(path)
            .map_err(|e| EngineError::Read { path: path.to_path_buf(), source: e })?;
        // Compiled outside the lock: it is the expensive call, and holding the mutex across
        // it stalls every other guest behind the first to load. Two callers racing both
        // compile and one result is dropped — cheaper than the contention, and the input is
        // a file that does not change under an install.
        let component = Component::new(&self.engine, &bytes)
            .map_err(EngineError::wasm(format!("compiling {}", path.display())))?;
        if let Ok(mut cache) = self.compiled.lock() {
            cache.entry(path.to_path_buf()).or_insert_with(|| component.clone());
        }
        Ok(component)
    }

    /// A fresh store for one guest, carrying its gate and its effects.
    pub fn store(&self, caps: GuestCaps, services: Services) -> Store<GuestState> {
        Store::new(&self.engine, GuestState::new(caps, services))
    }

    /// A linker with `arbor:host/*` wired in.
    pub fn linker(&self) -> Result<Linker<GuestState>, EngineError> {
        let mut linker = Linker::new(&self.engine);
        link_host(&mut linker)?;
        Ok(linker)
    }

    /// Drop every cached compilation.
    ///
    /// Called whenever the plugin set moves — an install, an uninstall, a reload. Wholesale
    /// rather than per-package because the alternative has an ordering hazard that is easy to
    /// get wrong and invisible when you do: knowing *which* modules a package owned means
    /// reading its manifest, and by the time an uninstall has finished the manifest is gone.
    ///
    /// The cost is bounded and small: what gets recompiled is what somebody actually opens
    /// next, and a module is a few hundred kilobytes.
    pub fn forget_all(&self) {
        if let Ok(mut cache) = self.compiled.lock() {
            cache.clear();
        }
    }
}

/// Generated bindings for the worlds in `wit/`.
///
/// `path` points at the **workspace's** `wit/` directory rather than a copy inside this
/// crate: those files are the public contract that third-party packages compile against, and
/// a second copy is a second thing to keep in step.
///
/// One generation per world, because a `bindgen!` call takes one. Only the cloud one is used
/// for **linking** — it imports all three host interfaces, and a linker is keyed by interface
/// name rather than by generated type, so registering them once satisfies any world that
/// imports a subset. The others exist only to call their exports.
pub mod bindings {
    pub mod cloud {
        wasmtime::component::bindgen!({
            path: "../../../../wit",
            world: "cloud-provider-world",
        });
    }

    pub mod studio {
        wasmtime::component::bindgen!({
            path: "../../../../wit",
            world: "studio-format-world",
        });
    }
}

use bindings::cloud::arbor::extensions as host_api;

// ── The host interfaces, implemented on the guest's state ───────────────────────
//
// Each one translates types and delegates to the `GuestState` method of the same name. The
// gate-then-effect ordering lives there, tested, and never here: a host function written
// directly against `services` would be a second place for that ordering to exist.

impl host_api::log::Host for GuestState {
    fn write(&mut self, lvl: host_api::log::Level, message: String) {
        let level = match lvl {
            host_api::log::Level::Debug => "debug",
            host_api::log::Level::Info => "info",
            host_api::log::Level::Warn => "warn",
            host_api::log::Level::Error => "error",
        };
        GuestState::log(self, level, &message);
    }
}

impl host_api::secrets::Host for GuestState {
    fn get(&mut self, key: String) -> Result<Option<String>, host_api::secrets::Error> {
        GuestState::credential_get(self, &key).map_err(secret_error)
    }

    fn set(&mut self, key: String, value: String) -> Result<(), host_api::secrets::Error> {
        GuestState::credential_set(self, &key, &value).map_err(secret_error)
    }

    fn delete(&mut self, key: String) -> Result<(), host_api::secrets::Error> {
        GuestState::credential_delete(self, &key).map_err(secret_error)
    }
}

/// Sort a credential failure into the interface's variants.
///
/// `undeclared` is the one worth telling apart: it means the package asked for a slot its
/// manifest never declared, and the fix is a manifest edit rather than anything about the
/// store. The message already says so — it is built by `arbor_plugin_types::credentials`,
/// beside the rule that refused it.
fn secret_error(message: String) -> host_api::secrets::Error {
    if message.contains("has no credential slot") {
        host_api::secrets::Error::Undeclared(message)
    } else if message.contains("no credential store") {
        host_api::secrets::Error::Unsupported
    } else {
        host_api::secrets::Error::Store(message)
    }
}

impl host_api::http::Host for GuestState {
    fn send(
        &mut self,
        req: host_api::http::Request,
    ) -> Result<host_api::http::Response, host_api::http::Error> {
        let out = GuestState::fetch(
            self,
            crate::services::HostRequest {
                method: req.method,
                url: req.url,
                headers: req.headers.into_iter().map(|h| (h.name, h.value)).collect(),
                body: req.body,
                timeout_secs: req.timeout_secs,
            },
        );
        match out {
            Ok(res) => Ok(host_api::http::Response {
                status: res.status,
                headers: res
                    .headers
                    .into_iter()
                    .map(|(name, value)| host_api::http::Header { name, value })
                    .collect(),
                body: res.body,
            }),
            // A denied host is not a transport failure and must not read as one: the request
            // was never made, and the fix is the manifest's allowlist.
            Err(e) if e.contains("network allowlist") || e.contains("requires `network`") => {
                Err(host_api::http::Error::NotAllowed(e))
            }
            Err(e) => Err(host_api::http::Error::Transport(e)),
        }
    }
}

/// Wire the host imports onto a linker.
pub fn link_host(linker: &mut Linker<GuestState>) -> Result<(), EngineError> {
    // WASI first, and empty. See `GuestState::wasi` for why it is here at all: the target
    // links it whether a guest uses it or not, so the choice is between an empty context and
    // a component that will not instantiate.
    wasmtime_wasi::add_to_linker_sync(linker)
        .map_err(EngineError::wasm("linking wasi"))?;
    host_api::log::add_to_linker(linker, |s: &mut GuestState| s)
        .map_err(EngineError::wasm("linking arbor:extensions/log"))?;
    host_api::secrets::add_to_linker(linker, |s: &mut GuestState| s)
        .map_err(EngineError::wasm("linking arbor:extensions/secrets"))?;
    host_api::http::add_to_linker(linker, |s: &mut GuestState| s)
        .map_err(EngineError::wasm("linking arbor:extensions/http"))?;
    Ok(())
}
