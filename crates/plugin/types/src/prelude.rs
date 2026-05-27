//! Canonical entry point for `arbor-plugin-types`' public API.
//!
//! Workspace convention: reach types through `arbor_plugin_types::prelude::…`
//! (or `use arbor_plugin_types::prelude::*;` at the top of a module) rather
//! than through the per-feature submodule paths. The submodules stay `pub`
//! for rustdoc navigation, but call sites should go through here.

pub use crate::dependency::{Dependency, LoadFailure};
pub use crate::hook_catalog::{FieldType, HookDef, HookField, HOOK_CATALOG, find};
pub use crate::hooks::Hooks;
pub use crate::manifest::{Manifest, ManifestParseError, ManifestParseFailure};
pub use crate::permissions::{
    AccessLevel, EnvReadPerm, GitLevel, Permissions, TerminalLevel,
};
pub use crate::sandbox::Sandbox;
pub use crate::schedule::{
    Schedule, ScheduleRegistry, ScheduleStatus, ScheduleTrigger, SchedulerSection,
    parse_duration_secs,
};
