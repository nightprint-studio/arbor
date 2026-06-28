//! Canonical entry point for `arbor-plugin-rpc`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_plugin_rpc::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the canonical path goes through here.

pub use crate::bundle::PluginRpc;
pub use crate::context::{OpenRepo, PluginRpcContext};
pub use crate::introspect::{DepGraphEdge, DepGraphNode};

// The generic handler logic, for backends (or tests) that want to call a single
// operation directly instead of going through the `PluginRpc` bundle.
pub use crate::dispatch::{exec_hook, fire_command, fire_plugin_action, set_active_tab};
pub use crate::introspect::{
    get_container, list_containers, list_contribution_points, list_plugin_contributions,
    list_plugin_info, plugin_dep_graph, plugin_dependents, plugin_disable_preview,
    plugin_enable_preview, plugin_settings_get, plugin_settings_set_all,
};
pub use crate::lifecycle::{disable_plugin, enable_plugin};
pub use crate::reload::{reload_plugins, reload_runtime, set_plugins_enabled};
pub use crate::scheduler::{start_plugin_scheduler, stop_plugin_scheduler};
