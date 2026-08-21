//! Canonical entry point for `arbor-plugin-types`' public API.
//!
//! Workspace convention: reach types through `arbor_plugin_types::prelude::…`
//! (or `use arbor_plugin_types::prelude::*;` at the top of a module) rather
//! than through the per-feature submodule paths. The submodules stay `pub`
//! for rustdoc navigation, but call sites should go through here.

pub use crate::dependency::{Dependency, LoadFailure};
pub use crate::hook_catalog::{
    self, FieldType, HookDef, HookField, HOOK_CATALOG, find, hooks_in_ns, is_known_namespace,
    resolve_subscription,
};
// Re-exported as modules, not flattened: a hook name is always read as
// `hook_names::corvus::COMMIT`, which is what makes the namespace visible at
// the call site. Flattening them would hide exactly the thing D9 added.
pub use crate::hook_names;
pub use crate::hook_ns;
pub use crate::hook_ns::{
    HOOK_NS_SEP, HOSTING_PRODUCTS, PRODUCT_ARBOR, PRODUCT_BENNU, PRODUCT_CORVUS, PRODUCT_GARRULUS,
    PRODUCT_LAUNCHER, PRODUCT_MERULA, PRODUCT_PICUS, PRODUCT_SITTA, PRODUCT_TYTO,
};
// The compile-time name builders. Exported here too so a product crate that
// declares its own namespace does not have to reach past the prelude.
pub use crate::{declare_hook_names, hook_name};
pub use crate::hooks::Hooks;
pub use crate::credentials::{
    account as credential_account, account_for as credential_account_for,
    belongs_to as credential_belongs_to, CredentialError, PLUGIN_PREFIX,
};
pub use crate::manifest::{Manifest, ManifestParseError, ManifestParseFailure};
pub use crate::network::{
    check as network_check, host_allowed, host_of, NetworkDenial,
};
pub use crate::provides::{CredentialSlot, LuaSection, Provides, WasmSection, WasmTarget};
pub use crate::permissions::{
    AccessLevel, EnvReadPerm, GitLevel, Permissions, RequiredPerm, TerminalLevel,
};
pub use crate::sandbox::Sandbox;
pub use crate::schedule::{
    Schedule, ScheduleRegistry, ScheduleStatus, ScheduleTrigger, SchedulerSection,
    parse_duration_secs,
};
