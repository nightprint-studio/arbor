//! `corvus-plugin` — the Corvus product's plugin-host wiring, shared between the
//! two processes that can host it.
//!
//! The mlua plugin host ([`arbor_plugin_core::prelude::PluginHost`]) is itself
//! Tauri-free and product-agnostic; what differs per host is the *wiring* around
//! it — how the hook dispatcher is built, which `arbor.*` surface is published,
//! and what `AppCtx` backs event egress. This crate holds the Corvus answers so
//! there is **one** definition the Tauri shell (in-process host) and the headless
//! `corvus-be` process (out-of-process host) both link:
//!
//! - [`dispatcher::build_hook_dispatcher`] — register the hook catalog + bind a
//!   `LuaHookListener` to a `PluginHost`. Identical in both processes.
//! - [`installer::CorvusBeApiInstaller`] — publish the `arbor.*` namespaces in a
//!   headless backend (host-pure base + the git/product namespaces it's handed).
//!
//! The generic headless `AppCtx` is `arbor_be::BackendAppCtx` (it has no Corvus
//! coupling — just an `EventSink` + Tokio runtime), so it lives in `arbor-be`.
//!
//! Public API: use the [`prelude`].

pub mod dispatcher;
pub mod installer;
pub mod prelude;
