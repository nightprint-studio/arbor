//! Host-pure `arbor.*` namespace installers. One module per top-level table
//! (or per closely-related cluster). These migrated out of
//! `src-tauri/src/plugin/api/ns/*` in PR #4 Step 6 because they depend only
//! on plugin-core state + the `AppCtx` capability surface — never on
//! src-tauri-internal types (`git::*`, `pipeline::*`, `jobs::*`, …).
//!
//! Namespaces that still need shell-side state stay in the Tauri crate and
//! are wired in as [`LuaNamespaceInstaller`](super::LuaNamespaceInstaller)
//! impls (see `src-tauri/src/plugin/api/mod.rs`).

pub(crate) mod command;
pub(crate) mod contribution;
pub(crate) mod credentials;
pub(crate) mod ext;
pub(crate) mod events;
pub(crate) mod fs;
pub(crate) mod hooks;
pub(crate) mod http;
pub(crate) mod json;
pub(crate) mod json_studio;
pub(crate) mod keybinding;
pub(crate) mod log;
pub(crate) mod meta;
pub(crate) mod notify;
pub(crate) mod oauth;
pub(crate) mod properties_studio;
pub(crate) mod ron_studio;
pub(crate) mod scheduler;
pub(crate) mod service;
pub(crate) mod settings;
pub(crate) mod text;
pub(crate) mod timer;
pub(crate) mod toml_studio;
pub(crate) mod ui;
pub(crate) mod yaml_studio;
