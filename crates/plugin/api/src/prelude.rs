//! Canonical entry point for `arbor-plugin-api`'s public API.
//!
//! Workspace convention: reach types through `arbor_plugin_api::prelude::…`
//! (or `use arbor_plugin_api::prelude::*;` at the top of a module) rather than
//! through the per-feature submodule paths. The submodules stay `pub` for
//! rustdoc navigation, but call sites should go through here.

pub use crate::ctx::PluginCtx;
pub use crate::dispatcher::{HookDispatcher, HookListener};
pub use crate::error::PluginError;
pub use crate::func::{NamespaceFn, PluginFn};
pub use crate::hook::{HookDef, HookKind};
pub use crate::namespace::NamespaceContributor;
pub use crate::perm::{
    ManifestPermError, PermReq, PermSchema, PermissionDef, PermissionsView,
};
pub use crate::registry::PluginRegistry;
pub use crate::value::{PluginMapExt, PluginValue};

// Re-export of the shared atoms from `arbor-plugin-types` so that
// contributors don't need a second `use` line just to spell a [`HookField`].
//
// `arbor_plugin_types::prelude::HookDef` is intentionally not re-exported:
// `arbor_plugin_api::prelude::HookDef` is the dynamic one (with
// [`HookKind`]) and reaching for the static catalog's `HookDef` is the
// niche case — the explicit `arbor_plugin_types::...` path on those rare
// sites makes the distinction loud.
pub use arbor_plugin_types::prelude::{FieldType, HookField, Manifest};
