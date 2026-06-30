//! `corvus-plugin-ns` — the Corvus git `ns_shell` namespaces (`arbor.repo`,
//! `arbor.notes`, …), ported to run inside **any** host through the [`NsHost`]
//! abstraction.
//!
//! ## Why this crate exists
//!
//! The shell's `ns_shell/*` installers reach into `tauri::AppState`, which pins
//! them to the Tauri shell process. To let plugins/hooks running inside the
//! headless `corvus-be` backend call the same `arbor.*` surface, each namespace
//! is reimplemented here as a `LuaNamespaceInstaller` that holds an
//! `Arc<dyn NsHost>` and calls coarse JSON-shaped methods on it instead of
//! downcasting an `AppState`. The Lua-visible behaviour is identical (same names,
//! arg shapes, return tuples, error strings).
//!
//! Light by design: depends only on `mlua` + `arbor-plugin-core` + `serde` — never
//! on `corvus-be` (a binary) nor on the heavy `git2`/provider crates. The host (the
//! `corvus-be` binary) implements [`NsHost`] over its own state + `corvus-git`.
//!
//! ## Layout
//!
//! - [`nshost`] — the [`NsHost`] host-abstraction trait (one method group per
//!   namespace).
//! - one module per ported `ns_shell` namespace, each exposing an
//!   `XInstaller` that holds an `Arc<dyn NsHost>`. Git/product namespaces
//!   ([`notes`], [`repo`], [`workspace`], [`linked_worktrees`], [`mr`], [`ci`],
//!   [`security`], [`issues`]) the host implements directly; platform namespaces
//!   ([`toolchain`], [`job`], [`ui_branding`]) the host implements by proxying to
//!   the shell over the reverse channel; [`tabs`] and [`terminal`] are emit /
//!   local-process direct.
//!
//! Public API is exposed through [`prelude`].
//!
//! See `docs/plugin-relocation-inventory.md` for the relocation context.

pub mod brp;
pub mod ci;
pub mod cloud;
pub mod issues;
pub mod job;
pub mod linked_worktrees;
pub mod mr;
pub mod notes;
pub mod nshost;
pub mod pipeline;
pub mod prelude;
pub mod repo;
pub mod security;
pub mod tabs;
pub mod terminal;
pub mod toolchain;
pub mod ui_branding;
pub mod workspace;

use std::sync::Arc;

use arbor_plugin_core::prelude::LuaNamespaceInstaller;

use crate::prelude::{
    BrpInstaller, CiInstaller, CloudInstaller, IssuesInstaller, JobInstaller,
    LinkedWorktreesInstaller, MrInstaller, NotesInstaller, NsHost, PipelineInstaller,
    RepoInstaller, SecurityInstaller, TabsInstaller, TerminalInstaller, ToolchainInstaller,
    UiBrandingInstaller, WorkspaceInstaller,
};

/// The ordered set of git/product namespace installers a host wires into its
/// plugin runtime, built over a shared [`NsHost`]. The order — and the invariant
/// that `UiBrandingInstaller` runs **after** the host-pure core namespaces (it
/// attaches onto the `arbor.ui` table `arbor_plugin_core`'s `ns::ui` publishes) —
/// is domain knowledge of these namespaces, so it lives here rather than in each
/// host's `main`. A host calls `register_lua_api(lua, params, &installers(host))`.
pub fn installers(host: Arc<dyn NsHost>) -> Vec<Arc<dyn LuaNamespaceInstaller>> {
    vec![
        Arc::new(NotesInstaller::new(host.clone())),
        Arc::new(RepoInstaller::new(host.clone())),
        Arc::new(WorkspaceInstaller::new(host.clone())),
        Arc::new(LinkedWorktreesInstaller::new(host.clone())),
        Arc::new(MrInstaller::new(host.clone())),
        Arc::new(CiInstaller::new(host.clone())),
        Arc::new(SecurityInstaller::new(host.clone())),
        Arc::new(ToolchainInstaller::new(host.clone())),
        // ── STAY ns_shell namespaces (DIRECT: work runs in the host process) ──
        Arc::new(TabsInstaller::new(host.clone())),
        Arc::new(IssuesInstaller::new(host.clone())),
        Arc::new(TerminalInstaller::new(host.clone())),
        // ── PROXY: state lives in the shell, reached over the reverse channel ──
        Arc::new(JobInstaller::new(host.clone())),
        Arc::new(PipelineInstaller::new(host.clone())),
        Arc::new(CloudInstaller::new(host.clone())),
        Arc::new(BrpInstaller::new(host.clone())),
        // UiBranding attaches onto the `arbor.ui` table the core install creates,
        // so it MUST come after the host-pure namespaces — keep it last.
        Arc::new(UiBrandingInstaller::new(host.clone())),
    ]
}
