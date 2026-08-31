//! Canonical entry point for `arbor-plugin-wasm`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `arbor_plugin_wasm::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::caps::GuestCaps;
pub use crate::guest::GuestState;
pub use crate::registry::{ExtensionEntry, ExtensionIndex, ExtensionKey, IndexProblem};
pub use crate::report::{ExtensionProblemRow, ExtensionRow, ExtensionsReport};
pub use crate::services::{HostRequest, HostResponse, HostServices, NoServices, Services};

#[cfg(feature = "runtime")]
pub use crate::dispatch::StudioGuest;
#[cfg(feature = "runtime")]
pub use crate::dynamic::{DynGuest, FuncSig, InterfaceSurface};
#[cfg(feature = "runtime")]
pub use crate::engine::{EngineError, WasmHost};
