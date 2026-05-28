//! Plugin runtime host: mlua VM management, lifecycle, sandbox, and the
//! built-in `arbor.*` Lua API surface.
//!
//! This crate is the actual machinery that loads, runs, and tears down
//! user-authored Lua plugins. It owns one [`mlua::Lua`] per loaded plugin,
//! the host-side state shared across plugins (contribution registry,
//! tree store, icon registry, toolchain state, settings store), the
//! lifecycle (`on_plugin_load` / `on_plugin_unload`), and the host-pure
//! slice of the `arbor.*` namespace (notify, fs, http, settings, ui.*,
//! studios, …).
//!
//! Namespaces that need src-tauri-internal concepts (`git::*`,
//! `pipeline::*`, `jobs::*`, `terminal::*`, `workspace::*`, `brp::*`,
//! `cloud::*`, …) stay in the Tauri shell crate as
//! [`LuaNamespaceInstaller`](lua_api::LuaNamespaceInstaller) implementations
//! and are wired into the runtime at boot. They migrate into their own
//! domain crates in PR #6+.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public
//! surface through a `prelude` module. Consumers either glob-import it once
//! per file (`use arbor_plugin_core::prelude::*;`) or fully qualify
//! (`arbor_plugin_core::prelude::PluginHost`). The per-feature submodules
//! stay `pub` for discoverability and rustdoc navigation, but call sites
//! should go through the prelude.
//!
//! See `docs/plugin-core-architecture.md` for the full PR #4 plan.

pub mod contribution;
pub mod error;
pub mod event_bus;
pub mod hook_router;
pub mod lua_api;
pub mod lua_ctx;
pub mod prelude;
pub mod runtime;
pub mod sandbox;
pub mod settings_store;
pub mod toolchain;
pub mod tree;
