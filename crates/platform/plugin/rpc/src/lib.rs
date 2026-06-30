//! `arbor-plugin-rpc` — the Plugin-Manager RPC surface for Model-D backends,
//! written once and shared by every product `*-be`.
//!
//! After the plugin product-relocation flip each product backend (`corvus-be`,
//! and later `merula-be` / `sitta-be`) owns the live `PluginHost` for its own
//! plugins, and re-serves the Plugin Manager's read + mutate + dispatch
//! operations as RPC handlers. That logic — enable/disable cascades, reload, the
//! master toggle, per-plugin schedulers, hook/command dispatch, and the whole
//! introspection/dep-graph surface — was identical across products. It lives here
//! now, generic over a [`prelude::PluginRpcContext`], with the three-times-
//! duplicated `with_host` helpers unified.
//!
//! ## How a product uses it (Bevy-like)
//!
//! 1. Define a **local adapter** over your state and `impl PluginRpcContext` for
//!    it (the orphan rule forbids implementing this foreign trait for a state
//!    type owned by another crate, so the adapter is a newtype in your binary).
//! 2. Add the [`prelude::PluginRpc`] bundle to your [`arbor_rpc::Builder`],
//!    monomorphised for that adapter, and dispatch the plugin methods with a `&`
//!    to it.
//!
//! No per-handler shims: the bundle's bodies are non-capturing closures that
//! coerce to the registry's fn-pointers (see `arbor_rpc::builder`).
//!
//! ## Public API: use the [`prelude`]

pub mod bundle;
pub mod context;
pub mod dispatch;
pub mod introspect;
pub mod lifecycle;
pub mod prelude;
pub mod reload;
pub mod scheduler;

pub use bundle::PluginRpc;
pub use context::{OpenRepo, PluginRpcContext};
