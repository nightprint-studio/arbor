//! Shell-side `arbor.*` namespace installers. These still depend on
//! src-tauri-internal types (`git::*`, `pipeline::*`, `jobs::*`,
//! `terminal::*`, `workspace::*`, `brp::*`, `cloud::*`, …) and stay here
//! until their domain crate is born (PR #6+).
//!
//! The host-pure namespaces (log, events, json, fs, http, ui.*, studios, …)
//! migrated into `arbor_plugin_core::lua_api::ns::*` in PR #4 Step 6.
//!
//! Each namespace is wrapped in a unit struct that adapts the legacy
//! `pub fn install(&ApiCtx, &Lua, &Table) -> AppResult<()>` signature into
//! the trait-required `-> PluginCoreResult<()>` (the only difference is the
//! error type, mapped via `to_string`). [`shell_installers`] returns them in
//! the order plugin-core invokes them — after the host-pure namespaces that
//! `arbor_plugin_core::lua_api::register` hardcodes, so `ui.branding` can
//! attach to the `arbor.ui` table `ns::ui` already published.
//!
//! As ns/* migrate into their own domain crate (PR #6+), their wrapper
//! disappears from [`shell_installers`] and the wrapper list shrinks to
//! empty.

pub(crate) mod brp;
pub(crate) mod ci;
pub(crate) mod cloud;
pub(crate) mod ctx_ext;
pub(crate) mod issues;
pub(crate) mod job;
pub(crate) mod linked_worktrees;
pub(crate) mod mr;
pub(crate) mod notes;
pub(crate) mod pipeline;
pub(crate) mod repo;
pub(crate) mod security;
pub(crate) mod tabs;
pub(crate) mod terminal;
pub(crate) mod toolchain;
pub(crate) mod ui;
pub(crate) mod workspace;

use std::sync::Arc;

use mlua::{Lua, Table};

use arbor_plugin_core::prelude::{
    ApiCtx, LuaNamespaceInstaller, PluginCoreError, PluginCoreResult,
};

use crate::error::Result;

/// Macro to declare a per-namespace `LuaNamespaceInstaller` wrapper. The
/// wrapper struct is a zero-sized type; its `install(...)` thin-wraps the
/// legacy `<ns>::install(&ctx, lua, arbor)` call and maps the host-side
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

ns_installer!(RepoInstaller,               repo::install);
ns_installer!(WorkspaceInstaller,          workspace::install);
ns_installer!(TabsInstaller,               tabs::install);
ns_installer!(LinkedWorktreesInstaller,    linked_worktrees::install);
ns_installer!(ToolchainInstaller,          toolchain::install);
ns_installer!(TerminalInstaller,           terminal::install);
ns_installer!(JobInstaller,                job::install);
ns_installer!(NotesInstaller,              notes::install);
ns_installer!(IssuesInstaller,             issues::install);
ns_installer!(UiBrandingInstaller,         ui::install);
ns_installer!(PipelineInstaller,           pipeline::install);
ns_installer!(MrInstaller,                 mr::install);
ns_installer!(CiInstaller,                 ci::install);
ns_installer!(SecurityInstaller,           security::install);
ns_installer!(CloudInstaller,              cloud::install);
ns_installer!(BrpInstaller,                brp::install);

/// Build the ordered list of shell-side `LuaNamespaceInstaller` wrappers —
/// the namespaces that still need src-tauri-internal types. They run after
/// the host-pure namespaces that `arbor_plugin_core::lua_api::register`
/// hardcodes, so `ui.branding` can attach to the `arbor.ui` table that
/// plugin-core's `ns::ui` already published.
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
