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

pub mod ci;
pub mod issues;
pub mod job;
pub mod linked_worktrees;
pub mod mr;
pub mod notes;
pub mod nshost;
pub mod prelude;
pub mod repo;
pub mod security;
pub mod tabs;
pub mod terminal;
pub mod toolchain;
pub mod ui_branding;
pub mod workspace;
