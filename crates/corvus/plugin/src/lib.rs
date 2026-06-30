//! `corvus-plugin` — the **Corvus product's** plugin API installer.
//!
//! The mlua plugin host ([`arbor_plugin_core::prelude::PluginHost`]) and the
//! product-agnostic hook-dispatcher builder
//! ([`arbor_plugin_core::prelude::build_hook_dispatcher`]) are both foundations.
//! What's genuinely Corvus-specific is *which `arbor.*` surface a headless host
//! publishes*:
//!
//! - [`installer::CorvusBeApiInstaller`] — publish the `arbor.*` namespaces in a
//!   headless backend (host-pure base + the git/product namespaces it's handed).
//!
//! The generic headless `AppCtx` is `arbor_be::BackendAppCtx` (no Corvus coupling
//! — just an `EventSink` + Tokio runtime), so it lives in `arbor-be`.
//!
//! Public API: use the [`prelude`].

pub mod installer;
pub mod prelude;
