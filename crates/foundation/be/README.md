# arbor-be

The Model-D backend runtime scaffold — the prologue every product `*-be` shares.

## Purpose

`corvus-be` (and later `merula-be` / `sitta-be`) all start the same way: build the
framed-stdio writer + event sink + reverse-channel caller + tokio runtime, build
the plugin host, wire the scheduler, run a few pre-serve inits, reload plugins
after the `Hello`, and serve the dispatch loop. That replicable boilerplate lives
here.

- **`BackendIo`** — the four framed-stdio pieces in one call:
  - `stdout` (the shared protocol `SharedWriter`),
  - `sink` (`FrameEventSink`, the product state's event egress),
  - `host` (`FrameHostCaller`, the reverse channel),
  - `rt` (the multi-thread tokio runtime).
- **`BackendAppCtx`** — the headless `AppCtx` (event egress + runtime spawn) every
  backend's `PluginHost` uses.
- **`App`** — the fluent runtime builder. `plugin_host(product_id, build_hooks)`
  builds and wires the **whole** plugin runtime in one call: the `PluginHost`
  (filtered to the product), its `BackendAppCtx`, the hook dispatcher (from the
  product's catalog builder), and the shared trigger engine. Then the product
  reads `sink` / `hooks` / `host_caller` off the app to build its state, hands
  back the API installer (`api_installer`), adds `init`s, and calls
  `run(dispatcher)` — which fires the inits, serves the loop, and (by default)
  reloads + starts schedulers once `Hello` is on the wire.
- **`Dispatcher`** — assembles the method routing from handler **groups** so the
  product never hand-rolls the maps, the method-name union, or the per-call
  context branching. `inventory(program)` adds the `#[handler]`s (dispatched with
  the primary `&S`); `group(map, make)` adds a bundle whose handlers downcast to
  their own adapter, built fresh per call. It carries both the advertised names
  and the dispatch fn into `App::run`.

## What stays in the product binary

The concrete state, the namespace/`NsHost` wiring, and the **dispatcher groups** —
they name the product's concrete context type(s) (the inventory's `&S`, each
bundle's adapter) to downcast the type-erased `&dyn Any`. The RPC *composition*
(assembling a bundle's handlers) is `arbor-rpc`'s `Builder`; `arbor-be` routes the
groups + runs the loop around them.

```rust
let mut app = arbor_be::App::new(arbor_be::BackendIo::new());
app.plugin_host("corvus", my_product::build_hook_dispatcher);

let state = Arc::new(MyState::new(app.sink()).with_hooks(app.hooks()));
app.api_installer(my_product::api_installer(/* namespaces over state */));

let dispatcher = arbor_be::Dispatcher::new(state.clone(), app.runtime_handle())
    .inventory("")                                  // #[handler]s, ctx = &MyState
    .group(my_rpc::methods(), {                      // a bundle with its own adapter
        let s = state.clone();
        move || MyRpcCtx::new(s.clone())
    });

app.init(move || some_registry::init(app.host_caller()));
app.run(dispatcher)?;
```

## Public API: use the prelude

`arbor_be::prelude::{BackendIo, BackendAppCtx, App, Dispatcher}`.

## Depends on

`arbor-ipc` (the framed-stdio transport + serve loop), `arbor-rpc` (the handler
registry + `CallFn` types the `Dispatcher` routes), `arbor-plugin-core`
(`PluginHost`), `arbor-plugin-api` (`HookDispatcher`), `arbor-scheduler`
(`Scheduler`), `arbor-core` (`AppCtx`), `tokio`, `serde_json`.

## Consumed by

The product backends (`corvus-be`; future `merula-be` / `sitta-be`).
