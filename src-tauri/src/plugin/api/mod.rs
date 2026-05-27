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

ns_installer!(LogInstaller,                ns::log::install);
ns_installer!(EventsInstaller,             ns::events::install);
ns_installer!(ServiceInstaller,            ns::service::install);
ns_installer!(JsonInstaller,               ns::json::install);
ns_installer!(JsonStudioInstaller,         ns::json_studio::install);
ns_installer!(RonStudioInstaller,          ns::ron_studio::install);
ns_installer!(TomlStudioInstaller,         ns::toml_studio::install);
ns_installer!(YamlStudioInstaller,         ns::yaml_studio::install);
ns_installer!(PropertiesStudioInstaller,   ns::properties_studio::install);
ns_installer!(FsInstaller,                 ns::fs::install);
ns_installer!(TextInstaller,               ns::text::install);
ns_installer!(RepoInstaller,               ns::repo::install);
ns_installer!(WorkspaceInstaller,          ns::workspace::install);
ns_installer!(TabsInstaller,               ns::tabs::install);
ns_installer!(LinkedWorktreesInstaller,    ns::linked_worktrees::install);
ns_installer!(MetaInstaller,               ns::meta::install);
ns_installer!(SettingsInstaller,           ns::settings::install);
ns_installer!(ToolchainInstaller,          ns::toolchain::install);
ns_installer!(TerminalInstaller,           ns::terminal::install);
ns_installer!(JobInstaller,                ns::job::install);
ns_installer!(HttpInstaller,               ns::http::install);
ns_installer!(NotesInstaller,              ns::notes::install);
ns_installer!(IssuesInstaller,             ns::issues::install);
ns_installer!(TimerInstaller,              ns::timer::install);
ns_installer!(SchedulerInstaller,          ns::scheduler::install);
ns_installer!(UiInstaller,                 ns::ui::install);
ns_installer!(KeybindingInstaller,         ns::keybinding::install);
ns_installer!(CommandInstaller,            ns::command::install);
ns_installer!(HooksInstaller,              ns::hooks::install);
ns_installer!(ContributionInstaller,       ns::contribution::install);
ns_installer!(NotifyInstaller,             ns::notify::install);
ns_installer!(PipelineInstaller,           ns::pipeline::install);
ns_installer!(MrInstaller,                 ns::mr::install);
ns_installer!(CiInstaller,                 ns::ci::install);
ns_installer!(SecurityInstaller,           ns::security::install);
ns_installer!(CloudInstaller,              ns::cloud::install);
ns_installer!(BrpInstaller,                ns::brp::install);

/// Build the ordered list of shell-side `LuaNamespaceInstaller` wrappers.
///
/// Order matches the legacy `register(...)` body verbatim — preserving it
/// matters in a few spots (`events` reads the `__arbor_hooks__` table
/// `lua_api::register` sets up first; `service` bootstraps its own globals;
/// `ui` consumers rely on the contribution registry being already alive).
///
/// As host-pure namespaces migrate into plugin-core (Step 6), their
/// entries drop off this list and `arbor_plugin_core::lua_api::register`
/// hardcodes them instead.
pub fn shell_installers() -> Vec<Arc<dyn LuaNamespaceInstaller>> {
    vec![
        Arc::new(LogInstaller),
        Arc::new(EventsInstaller),
        Arc::new(ServiceInstaller),
        Arc::new(JsonInstaller),
        Arc::new(JsonStudioInstaller),
        Arc::new(RonStudioInstaller),
        Arc::new(TomlStudioInstaller),
        Arc::new(YamlStudioInstaller),
        Arc::new(PropertiesStudioInstaller),
        Arc::new(FsInstaller),
        Arc::new(TextInstaller),
        Arc::new(RepoInstaller),
        Arc::new(WorkspaceInstaller),
        Arc::new(TabsInstaller),
        Arc::new(LinkedWorktreesInstaller),
        Arc::new(MetaInstaller),
        Arc::new(SettingsInstaller),
        Arc::new(ToolchainInstaller),
        Arc::new(TerminalInstaller),
        Arc::new(JobInstaller),
        Arc::new(HttpInstaller),
        Arc::new(NotesInstaller),
        Arc::new(IssuesInstaller),
        Arc::new(TimerInstaller),
        Arc::new(SchedulerInstaller),
        Arc::new(UiInstaller),
        Arc::new(KeybindingInstaller),
        Arc::new(CommandInstaller),
        Arc::new(HooksInstaller),
        Arc::new(ContributionInstaller),
        Arc::new(NotifyInstaller),
        Arc::new(PipelineInstaller),
        Arc::new(MrInstaller),
        Arc::new(CiInstaller),
        Arc::new(SecurityInstaller),
        Arc::new(CloudInstaller),
        Arc::new(BrpInstaller),
    ]
}
