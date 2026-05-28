//! `arbor.*` Lua API surface — Tauri-shell side.
//!
//! After PR #4 Step 5, the orchestrator (`register(...)`) and the shared
//! [`ApiCtx`] both live in [`arbor_plugin_core::lua_api`]. This module owns
//! the per-namespace installers that still need src-tauri-internal types
//! (`git::*`, `pipeline::*`, `jobs::*`, `terminal::*`, `workspace::*`,
//! `brp::*`, `cloud::*`, …) plus a thin shim that registers each of them
//! as a [`LuaNamespaceInstaller`] with the plugin-core registry.
//!
//! Each namespace is wrapped in a unit struct that adapts the legacy
//! `pub fn install(&ApiCtx, &Lua, &Table) -> AppResult<()>` signature into
//! the trait-required `-> PluginCoreResult<()>` (the only difference is
//! the error type, mapped via `to_string`). As ns/* migrate into
//! plugin-core in Step 6, their wrappers disappear from
//! [`shell_installers`] and `register(...)` in plugin-core gains a direct
//! hardcoded call instead.

pub(crate) mod ctx;
pub(crate) mod helpers;
mod ns;

use std::sync::Arc;

use mlua::{Lua, Table};

use arbor_plugin_core::error::PluginCoreError;
use arbor_plugin_core::prelude::{
    ApiCtx, LuaNamespaceInstaller, PluginCoreResult,
};

use crate::error::Result;

/// Macro to declare a per-namespace `LuaNamespaceInstaller` wrapper. The
/// wrapper struct is a zero-sized type; its `install(...)` thin-wraps the
/// legacy `ns::*::install(&ctx, lua, arbor)` call and maps the host-side
/// `AppError` into [`PluginCoreError::Plugin`].
macro_rules! ns_installer {
    ($name:ident, $path:path) => {
        pub(crate) struct $name;
        impl LuaNamespaceInstaller for $name {
            fn install(
                &self,
                ctx: &ApiCtx,
                lua: &Lua,
                arbor: &Table,
            ) -> PluginCoreResult<()> {
                let f: fn(&ApiCtx, &Lua, &Table) -> Result<()> = $path;
                f(ctx, lua, arbor)
                    .map_err(|e| PluginCoreError::Plugin(e.to_string()))
            }
        }
    };
}

ns_installer!(RepoInstaller,               ns::repo::install);
ns_installer!(WorkspaceInstaller,          ns::workspace::install);
ns_installer!(TabsInstaller,               ns::tabs::install);
ns_installer!(LinkedWorktreesInstaller,    ns::linked_worktrees::install);
ns_installer!(ToolchainInstaller,          ns::toolchain::install);
ns_installer!(TerminalInstaller,           ns::terminal::install);
ns_installer!(JobInstaller,                ns::job::install);
ns_installer!(NotesInstaller,              ns::notes::install);
ns_installer!(IssuesInstaller,             ns::issues::install);
ns_installer!(UiBrandingInstaller,         ns::ui::install);
ns_installer!(PipelineInstaller,           ns::pipeline::install);
ns_installer!(MrInstaller,                 ns::mr::install);
ns_installer!(CiInstaller,                 ns::ci::install);
ns_installer!(SecurityInstaller,           ns::security::install);
ns_installer!(CloudInstaller,              ns::cloud::install);
ns_installer!(BrpInstaller,                ns::brp::install);

/// Build the ordered list of shell-side `LuaNamespaceInstaller` wrappers —
/// the namespaces that still need src-tauri-internal types. They run after
/// the host-pure namespaces that `arbor_plugin_core::lua_api::register`
/// hardcodes, so `ui.branding` can attach to the `arbor.ui` table that
/// plugin-core's `ns::ui` already published.
///
/// The host-pure namespaces (log, events, json, fs, http, ui.*, studios,
/// notify, …) migrated into plugin-core in PR #4 Step 6 and are no longer
/// wrapped here.
pub fn shell_installers() -> Vec<Arc<dyn LuaNamespaceInstaller>> {
    vec![
        Arc::new(RepoInstaller),
        Arc::new(WorkspaceInstaller),
        Arc::new(TabsInstaller),
        Arc::new(LinkedWorktreesInstaller),
        Arc::new(ToolchainInstaller),
        Arc::new(TerminalInstaller),
        Arc::new(JobInstaller),
        Arc::new(NotesInstaller),
        Arc::new(IssuesInstaller),
        Arc::new(UiBrandingInstaller),
        Arc::new(PipelineInstaller),
        Arc::new(MrInstaller),
        Arc::new(CiInstaller),
        Arc::new(SecurityInstaller),
        Arc::new(CloudInstaller),
        Arc::new(BrpInstaller),
    ]
}
