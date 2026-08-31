//! Calling a guest.
//!
//! [`crate::engine`] loads and links; this is what the rest of Arbor actually holds. A
//! [`StudioGuest`] owns one instance and one `Store`, which is what keeps one guest's memory
//! and one guest's capabilities from being another's.
//!
//! ## Only what Arbor itself calls
//!
//! There was a `CloudGuest` here too, and removing it is the point rather than a tidy-up: a
//! typed wrapper in this crate means Arbor carries the interface, generates against it and
//! routes to it — which is a built-in with a wasm file attached, not an extension. The cloud
//! is a plugin's now, reached through the dynamic path
//! ([`crate::dynamic`], `arbor.ext.call`), and nothing about a bucket is spelled in Arbor.
//! `studio-format` is still here because it is genuinely Arbor's: the editor calls it.
//!
//! ## Every method here blocks
//!
//! Guests are synchronous by contract, so a call runs to completion on the calling thread —
//! and a host function it makes on the way (an HTTP request, a keychain read) blocks that
//! thread too.
//!
//! **Therefore every call into these types must come from `spawn_blocking`.** It is the same
//! landmine the whole backend is built around (`docs/backend-architecture.md`): occupying a
//! runtime worker with something that waits on a future that worker has to drive is the
//! deadlock that produces white windows.
//!
//! ## Why a guest is not shared
//!
//! Each of these owns a `Store`, and a `Store` is not `Sync`. That is not a limitation to work
//! around — it is the isolation. Two callers wanting the same backend get two instances, which
//! cost an instantiation rather than a compile because the component is cached.

use std::path::Path;

use wasmtime::Store;

use crate::caps::GuestCaps;
use crate::engine::{bindings, EngineError, WasmHost};
use crate::guest::GuestState;
use crate::registry::ExtensionEntry;
use crate::services::Services;

/// A live Studio format backend.
pub struct StudioGuest {
    store: Store<GuestState>,
    world: bindings::studio::StudioFormatWorld,
}

impl WasmHost {
    /// Bring up a Studio format backend.
    pub fn open_studio(
        &self,
        module: &Path,
        caps: GuestCaps,
        services: Services,
    ) -> Result<StudioGuest, EngineError> {
        let component = self.component(module)?;
        let linker = self.linker()?;
        let mut store = self.store(caps, services);
        let world = bindings::studio::StudioFormatWorld::instantiate(
            &mut store, &component, &linker,
        )
        .map_err(EngineError::wasm(format!("instantiating {}", module.display())))?;
        Ok(StudioGuest { store, world })
    }

    /// Bring up whichever guest an index entry names, for a caller that only has the entry.
    ///
    /// Returns the module path back so the caller can `forget` it on uninstall without
    /// re-deriving it from the manifest.
    pub fn module_of(entry: &ExtensionEntry) -> &Path {
        &entry.module
    }
}

impl StudioGuest {
    /// The world's exported interface and the store it runs in.
    ///
    /// Handed out as a pair rather than wrapped call by call: `studio-format` is thirteen
    /// functions over a resource, the shapes come straight from the WIT, and a wrapper each
    /// would be thirteen functions whose only content is passing two arguments along.
    pub fn parts(
        &mut self,
    ) -> (&mut Store<GuestState>, &bindings::studio::exports::arbor::extensions::studio_format::Guest)
    {
        (&mut self.store, self.world.arbor_extensions_studio_format())
    }
}
