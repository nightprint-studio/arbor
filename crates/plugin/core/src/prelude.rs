//! Canonical entry point for `arbor-plugin-core`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers should reach types and
//! functions through `arbor_plugin_core::prelude::...` (or a single
//! `use arbor_plugin_core::prelude::*;` at the top of a module) rather
//! than through the per-feature submodule paths. The submodules stay
//! `pub` for rustdoc navigation, but call sites should go through here.
//!
//! Entries land as each step of PR #4 atterra. See
//! `docs/plugin-core-architecture.md` for the migration plan.

// ── Cross-plugin primitives (Step 3) ────────────────────────────────────────
pub use crate::contribution::{
    ContainerDef, ContributionPoint, ContributionRegistry, EventCoalescer,
    PluginContribution, WhenClause, DataFieldMatch, StringOrVec,
    points, payloads, validate_built_in,
};
pub use crate::lua_ctx::{install as install_lua_ctx, record as record_lua, PluginLuaCtx};
pub use crate::settings_store::{
    SettingsMap, global_settings_path, load_settings_file, project_settings_path,
    save_settings_file,
};
pub use crate::toolchain::{ToolchainEntry, ToolchainRegistry, toolchains_dir};
pub use crate::tree::{
    BreadcrumbSegment, IconRegistry, TreeNode, TreeSnapshot, TreeStore,
};
