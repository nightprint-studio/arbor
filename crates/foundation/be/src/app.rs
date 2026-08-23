//! `App` — the backend runtime builder. Owns the generic Model-D boilerplate so a
//! product `main` stays declarative.
//!
//! [`plugin_host`](App::plugin_host) builds and wires the whole plugin runtime in
//! one call — the `PluginHost`, its [`BackendAppCtx`], the product filter, the
//! hook dispatcher (via the product's catalog builder), and the shared trigger
//! engine. The product then reads `sink` / `hooks` / `host_caller` off the app to
//! build its own state, hands back the API installer + advertised methods +
//! dispatch, and calls [`run`](App::run).
//!
//! What stays in the product binary (it names product types): the concrete state,
//! the namespace/`NsHost` wiring, and the **dispatch closure** (it downcasts the
//! type-erased `&dyn Any` to the concrete context). Everything else lives here.

use std::io;
use std::sync::{Arc, Mutex};

use arbor_core::prelude::AppCtx;
use arbor_ipc::prelude::{serve_stdio, EventSink, HostCaller};
use arbor_plugin_api::prelude::HookDispatcher;
use arbor_plugin_core::prelude::{LuaApiInstaller, PluginHost};
use arbor_scheduler::prelude::Scheduler;

use crate::app_ctx::BackendAppCtx;
use crate::dispatch::Dispatcher;
use crate::io::BackendIo;

/// Fluent builder around a [`BackendIo`] that wires + runs a Model-D backend.
pub struct App {
    io: BackendIo,
    plugin_host: Option<Arc<Mutex<PluginHost>>>,
    hooks: Option<Arc<HookDispatcher>>,
    inits: Vec<Box<dyn FnOnce()>>,
    on_ready: Option<Box<dyn FnOnce()>>,
}

impl App {
    /// Start an app over the given IO.
    pub fn new(io: BackendIo) -> Self {
        Self { io, plugin_host: None, hooks: None, inits: Vec::new(), on_ready: None }
    }

    /// Build and wire the plugin runtime: a `PluginHost` filtered to `product_id`
    /// (plugins targeting it or universal), backed by a [`BackendAppCtx`] over this
    /// IO, with the hook dispatcher `build_hooks` produces (e.g. the product's
    /// `build_hook_dispatcher`) and the shared trigger engine installed. After
    /// this, [`hooks`](Self::hooks) / [`plugin_host_handle`](Self::plugin_host_handle)
    /// are available for the product to build its state + adapter.
    ///
    /// The host scans both of the profile's plugin pools on its own — see
    /// `arbor_plugin_core`'s `plugin_roots`. No backend has to name a root.
    pub fn plugin_host<H>(&mut self, product_id: &str, build_hooks: H) -> &mut Self
    where
        H: FnOnce(&Arc<Mutex<PluginHost>>) -> HookDispatcher,
    {
        let host = Arc::new(Mutex::new(PluginHost::new()));
        // Wire the reverse channel so UI capabilities the backend can't satisfy
        // locally (`open_path`) forward to the shell's host handlers.
        let app_ctx: Arc<dyn AppCtx> = Arc::new(
            BackendAppCtx::new(self.io.sink(), self.io.runtime_handle())
                .with_host_caller(self.io.host_caller()),
        );
        {
            let mut h = host.lock().expect("arbor-be: plugin host poisoned at boot");
            h.set_app_ctx(app_ctx.clone());
            h.set_product(product_id);
        }
        // Built from the host (the catalog builder binds a listener to it), held by
        // the product state to fire hooks onto the same dispatcher.
        let hooks = Arc::new(build_hooks(&host));
        // Shared trigger engine, so `product_id`-targeted plugins' schedules fire
        // here. Installed now; started after the `Hello` reload (see `run`).
        let scheduler = Arc::new(Scheduler::new(app_ctx, self.io.runtime_handle()));
        host.lock()
            .expect("arbor-be: plugin host poisoned at scheduler install")
            .install_scheduler(scheduler, Arc::downgrade(&host));
        self.plugin_host = Some(host);
        self.hooks = Some(hooks);
        self
    }

    /// A clone of the event sink (the product state's egress).
    pub fn sink(&self) -> Arc<dyn EventSink> {
        self.io.sink()
    }

    /// The reverse-channel caller as a trait object (state + credential registries).
    pub fn host_caller(&self) -> Arc<dyn HostCaller> {
        self.io.host_caller()
    }

    /// A handle to the runtime (e.g. for the dispatch closure's async `block_on`).
    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.io.runtime_handle()
    }

    /// The hook dispatcher built in [`plugin_host`](Self::plugin_host) — the
    /// product state holds it to fire hooks. Call `plugin_host` first.
    pub fn hooks(&self) -> Arc<HookDispatcher> {
        Arc::clone(
            self.hooks
                .as_ref()
                .expect("arbor-be: call .plugin_host(...) before .hooks()"),
        )
    }

    /// The plugin host handle (e.g. to publish it for an RPC adapter). Call
    /// `plugin_host` first.
    pub fn plugin_host_handle(&self) -> Arc<Mutex<PluginHost>> {
        Arc::clone(
            self.plugin_host
                .as_ref()
                .expect("arbor-be: call .plugin_host(...) first"),
        )
    }

    /// Set the host's `arbor.*` API installer (the product's namespaces). Call
    /// `plugin_host` first.
    pub fn api_installer(&mut self, installer: Arc<dyn LuaApiInstaller>) -> &mut Self {
        self.plugin_host
            .as_ref()
            .expect("arbor-be: call .plugin_host(...) before .api_installer(...)")
            .lock()
            .expect("arbor-be: plugin host poisoned at api-installer set")
            .set_api_installer(installer);
        self
    }

    /// Add a pre-serve init step (registries, git detect, …). Inits run in order,
    /// once, immediately before the serve loop starts.
    pub fn init(&mut self, f: impl FnOnce() + 'static) -> &mut Self {
        self.inits.push(Box::new(f));
        self
    }

    /// Override the post-`Hello` startup hook. The default (when unset) reloads the
    /// plugin runtime and starts its schedulers. Runs strictly **after** `Hello` is
    /// on the wire (on-load hooks emit events, which must not precede the
    /// handshake frame).
    pub fn on_ready(&mut self, f: impl FnOnce() + 'static) -> &mut Self {
        self.on_ready = Some(Box::new(f));
        self
    }

    /// Fire the inits, then serve `dispatcher` over framed stdio until the shell
    /// disconnects. The advertised method set (`Hello`) and the dispatch routing
    /// both come from the [`Dispatcher`] — the product only declares its handler
    /// groups (see [`Dispatcher::inventory`] / [`Dispatcher::group`]).
    pub fn run<S>(self, dispatcher: Dispatcher<S>) -> io::Result<()>
    where
        S: Send + Sync + 'static,
    {
        for init in self.inits {
            init();
        }
        let methods = dispatcher.methods();
        eprintln!("arbor-be: ready, serving {} method(s): {:?}", methods.len(), methods);
        let dispatch = dispatcher.into_fn();
        // Default post-`Hello` hook: rebuild the plugin runtime + start its
        // schedulers, on a BACKGROUND thread so the serve loop starts immediately.
        //
        // `serve_stdio` runs `on_ready` inline before it begins reading frames, so
        // anything slow here stalls the read loop — and the shell blocks on this
        // backend's first responses right after `Hello`: `ensure_corvus_be` calls
        // `sync_config` (config push) the moment it attaches. A plugin load that is
        // merely slow, or that re-enters the shell from an `on_load` host_call,
        // would then deadlock the shell and, downstream, keep the product window
        // from ever opening. Spawning keeps the read loop responsive; on-load
        // events still follow `Hello` (already written before this spawn), and the
        // plugin host is `Send` (mlua `send` feature) so the `Arc<Mutex<…>>` moves
        // across the thread boundary. The frontend re-queries contributions when
        // they land (`arbor://plugins-reloaded` / contribution events).
        let ph = self.plugin_host.clone();
        let on_ready: Box<dyn FnOnce()> = self.on_ready.unwrap_or_else(move || {
            Box::new(move || {
                if let Some(ph) = ph {
                    std::thread::spawn(move || {
                        let mut host = ph.lock().unwrap_or_else(|p| p.into_inner());
                        if let Err(e) = host.reload() {
                            eprintln!("arbor-be: plugin reload failed: {e}");
                        }
                        host.start_all_schedulers();
                    });
                }
            })
        });
        // `self.io.rt` stays owned by `self` (alive across the serve loop) while
        // `stdout` / `host` move into it — handlers hold `Handle`s into rt.
        serve_stdio(
            io::stdin().lock(),
            self.io.stdout,
            methods,
            self.io.host,
            dispatch,
            on_ready,
        )
    }
}
