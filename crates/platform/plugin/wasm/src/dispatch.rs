//! Calling a guest.
//!
//! [`crate::engine`] loads and links; this is what the rest of Arbor actually holds. A
//! [`StudioGuest`] or a [`CloudGuest`] owns one instance and one `Store`, which is what keeps
//! one guest's memory and one guest's capabilities from being another's.
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

/// A live cloud provider, bound to one bucket.
pub struct CloudGuest {
    store: Store<GuestState>,
    world: bindings::cloud::CloudProviderWorld,
    /// The open connection. `None` until [`CloudGuest::connect`] succeeds — every operation
    /// needs one, and a guest that was instantiated but never connected is a real state (the
    /// bucket did not exist, the token was refused).
    connection: Option<Connection>,
}

use bindings::cloud::exports::arbor::extensions::cloud_provider::Connection;

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

    /// Bring up a cloud provider.
    pub fn open_cloud(
        &self,
        module: &Path,
        caps: GuestCaps,
        services: Services,
    ) -> Result<CloudGuest, EngineError> {
        let component = self.component(module)?;
        let linker = self.linker()?;
        let mut store = self.store(caps, services);
        let world = bindings::cloud::CloudProviderWorld::instantiate(
            &mut store, &component, &linker,
        )
        .map_err(EngineError::wasm(format!("instantiating {}", module.display())))?;
        Ok(CloudGuest { store, world, connection: None })
    }

    /// Bring up whichever guest an index entry names, for a caller that only has the entry.
    ///
    /// Returns the module path back so the caller can `forget` it on uninstall without
    /// re-deriving it from the manifest.
    pub fn module_of(entry: &ExtensionEntry) -> &Path {
        &entry.module
    }
}

/// The generated cloud types, re-exported so a host can name them without reaching into the
/// bindings module. These are the interface's own shapes — anything else would be a second
/// definition of an object listing.
pub use crate::engine::bindings::cloud::arbor::extensions::cloud_types::{
    Error as CloudError, Listing, Object as GuestObject, Range,
};

impl StudioGuest {
    /// The world's exported interface and the store it runs in.
    ///
    /// Handed out as a pair rather than wrapped call by call: `studio-format` is thirteen
    /// functions over a resource, the shapes come straight from the WIT, and a wrapper each
    /// would be thirteen functions whose only content is passing two arguments along. The
    /// cloud side is wrapped because it is small enough to be worth hiding.
    pub fn parts(
        &mut self,
    ) -> (&mut Store<GuestState>, &bindings::studio::exports::arbor::extensions::studio_format::Guest)
    {
        (&mut self.store, self.world.arbor_extensions_studio_format())
    }
}

impl CloudGuest {
    /// Open the connection this guest will serve.
    ///
    /// Separate from instantiation because authentication is: a token fetch happens once and
    /// is reused, and an interface where every call carried its bucket would redo it on every
    /// keystroke of a directory listing.
    pub fn connect(&mut self, bucket: &str, config: &str) -> Result<(), EngineError> {
        let iface = self.world.arbor_extensions_cloud_provider();
        let conn = iface
            .connection()
            .call_open(&mut self.store, bucket, config)
            .map_err(EngineError::wasm("opening a connection"))?
            .map_err(|e| EngineError::Guest(format!("{e:?}")))?;
        self.connection = Some(conn);
        Ok(())
    }

    /// Bind a bucket with an empty config — the shape a provider that needs nothing else uses.
    pub fn set_bucket(&mut self, bucket: String) {
        if let Err(e) = self.connect(&bucket, "") {
            tracing::warn!("cloud guest: could not open '{bucket}': {e}");
        }
    }

    fn conn(&self) -> Result<&Connection, String> {
        self.connection
            .as_ref()
            .ok_or_else(|| "cloud guest: no connection was opened".to_string())
    }

    pub fn test(&mut self) -> Result<(), String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_test(&mut self.store, c)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    pub fn list(
        &mut self,
        prefix: &str,
        delimiter: Option<&str>,
        cursor: Option<String>,
        limit: Option<u32>,
    ) -> Result<Listing, String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_list(&mut self.store, c, prefix, delimiter, cursor.as_deref(), limit)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    pub fn stat(&mut self, key: &str) -> Result<GuestObject, String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_stat(&mut self.store, c, key)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    /// `Ok(None)` for a key that is not there.
    ///
    /// The interface separates `not-found` from every other failure precisely so a caller can
    /// ask "is this there?" without reading a message, and [`Self::stat`] flattening every
    /// variant into a string left the one caller that needs the distinction — the overwrite
    /// check on an upload — guessing from prose.
    pub fn stat_opt(&mut self, key: &str) -> Result<Option<GuestObject>, String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        match iface
            .connection()
            .call_stat(&mut self.store, c, key)
            .map_err(|e| e.to_string())?
        {
            Ok(o) => Ok(Some(o)),
            Err(CloudError::NotFound(_)) => Ok(None),
            Err(e) => Err(format!("{e:?}")),
        }
    }

    pub fn read(&mut self, key: &str, part: Option<Range>) -> Result<Vec<u8>, String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_read(&mut self.store, c, key, part)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    pub fn write(
        &mut self,
        key: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> Result<(), String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_write(&mut self.store, c, key, body, content_type)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    pub fn delete(&mut self, key: &str) -> Result<(), String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_delete(&mut self.store, c, key)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }

    pub fn copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        let c = *self.conn()?;
        let iface = self.world.arbor_extensions_cloud_provider();
        iface
            .connection()
            .call_copy(&mut self.store, c, source, destination)
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("{e:?}"))
    }
}
