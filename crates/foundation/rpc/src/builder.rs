//! Runtime composition of the dispatch surface — the Bevy-like counterpart to
//! the compile-time `#[handler]`/`inventory` path.
//!
//! `#[handler]` is ideal for a backend's **own** concrete handlers, but it can't
//! register **generic** ones: the macro's `inventory::submit!` bakes a concrete
//! context downcast, and inventory entries are non-generic and link-local. The
//! [`Builder`] adds the missing capability — a backend assembles its dispatch by
//! pulling its `#[handler]` set ([`add_inventory`](Builder::add_inventory)) and
//! adding reusable [`RpcBundle`]s ([`add`](Builder::add)): generic handler groups
//! monomorphised for the backend's context `C` at the call site. Both feed the
//! same method → [`CallFn`] / [`AsyncCallFn`] maps the serve loop dispatches
//! against, so the macro path and the bundle path coexist (the "hybrid" model).
//!
//! ## Why bundles can be generic when `#[handler]` can't
//!
//! A bundle's bodies are **non-capturing** closures that reference only the type
//! `C` (to `downcast_ref::<C>()` the type-erased context). A non-capturing
//! closure coerces to a plain fn-pointer, and the reference to `C` is resolved by
//! monomorphisation at the `.add::<C>(…)` call site — so the closure becomes a
//! concrete [`CallFn`] with no inventory and no per-handler glue in the product.

use std::collections::HashMap;
use std::marker::PhantomData;

use crate::{async_registry_for, registry_for, AsyncCallFn, CallFn, Kind};

/// One handler in an [`RpcBundle`]: a method name + its (sync/async) body.
///
/// Unlike the inventory [`Entry`](crate::Entry), a bundle entry carries no
/// `program` — it is added explicitly to a [`Builder`], not collected globally.
pub struct HandlerEntry {
    pub name: &'static str,
    pub kind: Kind,
}

impl HandlerEntry {
    /// A synchronous handler (`fn(&dyn Any, params) -> Result<Value, String>`).
    pub fn sync(name: &'static str, f: CallFn) -> Self {
        Self { name, kind: Kind::Sync(f) }
    }

    /// An asynchronous handler — awaited by the host on its runtime.
    pub fn async_fn(name: &'static str, f: AsyncCallFn) -> Self {
        Self { name, kind: Kind::Async(f) }
    }
}

/// A reusable group of handlers, generic over the backend context `C` — the
/// Bevy-`Plugin` of the dispatch world. A library exposes a bundle once; each
/// product adds it with a single [`Builder::add`] call, monomorphised for its own
/// context type.
pub trait RpcBundle<C: 'static> {
    /// The handlers this bundle contributes. Implementations build the entries
    /// from non-capturing closures that downcast `&dyn Any` to `&C` (see the
    /// module docs for why that keeps them generic yet fn-pointer-compatible).
    fn handlers(&self) -> Vec<HandlerEntry>;
}

/// Assembles a backend's dispatch surface from its `#[handler]` inventory set
/// plus any [`RpcBundle`]s, into the sync/async method maps the serve loop uses.
///
/// `C` is the backend's context type — every handler this builder collects is
/// dispatched with a `&C` (the inventory handlers were generated for it; the
/// bundle handlers are monomorphised for it).
pub struct Builder<C: 'static> {
    sync: HashMap<&'static str, CallFn>,
    asyncs: HashMap<&'static str, AsyncCallFn>,
    // `fn(&C)` keeps the param without imposing `Send`/`Sync` on `C` or owning one.
    _ctx: PhantomData<fn(&C)>,
}

impl<C: 'static> Default for Builder<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: 'static> Builder<C> {
    /// An empty builder.
    pub fn new() -> Self {
        Self { sync: HashMap::new(), asyncs: HashMap::new(), _ctx: PhantomData }
    }

    /// Pull in every `#[handler]` registered under `program` (sync + async). Use
    /// `""` for the default/bare-`#[handler]` program. This is the hybrid seam:
    /// concrete product handlers via the macro here, generic ones via
    /// [`add`](Self::add).
    pub fn add_inventory(mut self, program: &str) -> Self {
        for (n, f) in registry_for(program) {
            self.sync.insert(n, f);
        }
        for (n, f) in async_registry_for(program) {
            self.asyncs.insert(n, f);
        }
        self
    }

    /// Add a reusable [`RpcBundle`], monomorphised for `C`. A later add of the
    /// same method name overrides the earlier one (last-wins), so a product can
    /// shadow a bundled handler with its own.
    // Builder method intentionally named `add` (reads `builder.add(bundle)`); it
    // is not an arithmetic `+` and does not warrant implementing `std::ops::Add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, bundle: impl RpcBundle<C>) -> Self {
        for e in bundle.handlers() {
            match e.kind {
                Kind::Sync(f) => {
                    self.sync.insert(e.name, f);
                }
                Kind::Async(f) => {
                    self.asyncs.insert(e.name, f);
                }
            }
        }
        self
    }

    /// The assembled sync method map.
    pub fn sync_map(&self) -> &HashMap<&'static str, CallFn> {
        &self.sync
    }

    /// The assembled async method map.
    pub fn async_map(&self) -> &HashMap<&'static str, AsyncCallFn> {
        &self.asyncs
    }

    /// Every advertised method name (sync ∪ async), sorted + deduped — the set a
    /// backend announces in its `Hello`.
    pub fn methods(&self) -> Vec<String> {
        let mut m: Vec<String> = self
            .sync
            .keys()
            .chain(self.asyncs.keys())
            .map(|s| s.to_string())
            .collect();
        m.sort();
        m.dedup();
        m
    }

    /// Consume the builder into its `(sync, async)` method maps.
    pub fn into_maps(self) -> (HashMap<&'static str, CallFn>, HashMap<&'static str, AsyncCallFn>) {
        (self.sync, self.asyncs)
    }
}
