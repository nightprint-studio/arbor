//! Canonical entry point for `arbor-plugin-core`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers should reach types and
//! functions through `arbor_plugin_core::prelude::...` (or a single
//! `use arbor_plugin_core::prelude::*;` at the top of a module) rather
//! than through the per-feature submodule paths. The submodules stay
//! `pub` for rustdoc navigation, but call sites should go through here.
//!
//! This is the complete public surface of the crate as of PR #4. See
//! `docs/plugin-core-architecture.md` for the migration history.

// ── Cross-plugin primitives (Step 3) ────────────────────────────────────────
pub use crate::contribution::{
    ContainerDef, ContributionPoint, ContributionRegistry, EventCoalescer,
    PluginContribution, WhenClause, DataFieldMatch, StringOrVec,
    points, payloads, validate_built_in,
};
pub use crate::lua_ctx::{install as install_lua_ctx, record as record_lua, PluginLuaCtx};
pub use crate::settings_store::{
    SettingsMap, global_settings_path, load_settings_file, plugin_data_dir,
    project_settings_path, save_settings_file,
};
pub use crate::toolchain::{ToolchainEntry, ToolchainRegistry, toolchains_dir};
pub use crate::tree::{
    BreadcrumbSegment, IconRegistry, TreeNode, TreeSnapshot, TreeStore, TreeUpdate,
};

// ── Errors (Step 4) ─────────────────────────────────────────────────────────
pub use crate::error::{PluginCoreError, Result as PluginCoreResult};

// ── Sandbox + runtime (Step 4) ──────────────────────────────────────────────
pub use crate::sandbox::{
    create_sandbox, ApiInstallParams, LuaApiInstaller, NoopApiInstaller,
};
pub use crate::runtime::{
    ARBOR_API_VERSION, ARBOR_APP_VERSION, current_os,
    discover_in_roots, discover_plugins, load_plugin_states, plugin_dir,
    save_plugin_states,
    ComboOption, PluginInfo,
    DormantPlugin, LoadedPlugin, TimerCancel, TimerCancels, TimerCounter,
    PluginHost, load_plugin, PipelineOpResult, ServiceError, CommandError,
    host_command_required,
};
pub use crate::runtime::host::dep_cascade::{EnableBlocker, EnablePreview};

// ── Hook routing (Step 8) ───────────────────────────────────────────────────
pub use crate::hook_router::{
    fire, fire_broadcast, fire_collecting, fire_on, fire_vetoable, matches_pattern,
    LuaHookListener,
};
pub use crate::dispatcher::build_hook_dispatcher;

// ── Lua API surface (Step 5) ────────────────────────────────────────────────
pub use crate::lua_api::{
    register as register_lua_api, ApiCtx, LuaNamespaceInstaller,
};

// ── Lua API helpers (consumed by the shell-side `ns_shell/*` installers) ─────
pub use crate::lua_api::helpers::convert::json_to_lua;
pub use crate::lua_api::helpers::tuple::{boolerr2, err2, ok2, LuaTuple};
