//! Plugin-facing function: the unit a domain crate contributes to a namespace.
//!
//! Decision **D6**: function bodies are async (`#[async_trait]`). Most domain
//! work is naturally async — HTTP, GraphQL, libgit2 wrapped on a thread pool,
//! schedulers — and mlua's `async` feature lets the Lua runtime await the
//! resolution. CPU-only functions just return ready futures and pay nothing.

use std::sync::Arc;

use async_trait::async_trait;

use crate::ctx::PluginCtx;
use crate::error::PluginError;
use crate::perm::PermReq;
use crate::value::PluginValue;

/// The async body of a contributed function.
///
/// Object-safe via `async_trait` — the registry stores `Arc<dyn PluginFn>` so
/// the same body can be reused across runtimes (one mlua adapter calls it,
/// tomorrow a wasm adapter calls it, same Arc).
#[async_trait]
pub trait PluginFn: Send + Sync {
    async fn call(
        &self,
        ctx:  &(dyn PluginCtx + Sync),
        args: PluginValue,
    ) -> Result<PluginValue, PluginError>;
}

/// Entry in a namespace: identifies the function, lists its permission
/// requirements, holds the shared body.
///
/// The `namespace` + `name` pair is what the script side sees (Lua's
/// `arbor.<namespace>.<name>(...)`); the registry uses them as the lookup key.
pub struct NamespaceFn {
    pub namespace: &'static str,
    pub name:      &'static str,
    /// Predicates the registry checks **before** invoking `body`. AND-combined.
    pub requires:  &'static [PermReq],
    pub body:      Arc<dyn PluginFn>,
}
