//! Shell-side `arbor.*` namespace installers. These still depend on
//! src-tauri-internal types (`git::*`, `pipeline::*`, `jobs::*`,
//! `terminal::*`, `workspace::*`, `brp::*`, `cloud::*`, …) and stay here
//! until their domain crate is born (PR #6+).
//!
//! The host-pure namespaces (log, events, json, fs, http, ui.*, studios, …)
//! migrated into `arbor_plugin_core::lua_api::ns::*` in PR #4 Step 6.

pub(crate) mod brp;
pub(crate) mod ci;
pub(crate) mod cloud;
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
