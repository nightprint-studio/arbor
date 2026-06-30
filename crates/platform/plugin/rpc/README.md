# arbor-plugin-rpc

The Plugin-Manager RPC surface for Model-D backends — written once, shared by
every product `*-be`.

## Purpose

After the plugin product-relocation flip, each product backend (`corvus-be`,
later `merula-be` / `sitta-be`) owns the live `PluginHost` for its plugins and
re-serves the Plugin Manager's operations as RPC handlers. That logic was
identical across products; it lives here now, generic over a `PluginRpcContext`,
with the previously-triplicated `with_host` helpers unified.

Operations covered: enable/disable (with the transitive cascade), reload, the
master kill-switch, per-plugin scheduler start/stop, hook/command dispatch +
`set_active_tab`, and the read surface (plugin info, dep-graph, contributions,
containers, settings get/set).

## How a product uses it (Bevy-like)

1. Define a **local adapter** over your backend state and `impl PluginRpcContext`
   for it. (The orphan rule forbids implementing this foreign trait for a state
   type owned by another crate, so the adapter is a newtype in your binary.)
2. Add the `PluginRpc` bundle to your `arbor_rpc::Builder`, monomorphised for the
   adapter, and dispatch the plugin methods with a `&` to it:

```rust
let (sync, _async) = arbor_rpc::Builder::<MyRpcCtx>::new()
    .add(arbor_plugin_rpc::PluginRpc)
    .into_maps();
```

No per-handler shims: the bundle's bodies are non-capturing closures that coerce
to the registry's fn-pointers (see `arbor_rpc::builder`).

## Public API: use the prelude

Reach the surface through `arbor_plugin_rpc::prelude::...`: `PluginRpcContext`,
`OpenRepo`, `PluginRpc`, `DepGraphNode`/`DepGraphEdge`, and every generic handler
function (`enable_plugin`, `reload_plugins`, `list_plugin_info`, …) for backends
that want to call one directly instead of through the bundle.

## Depends on

`arbor-rpc` (the dispatch core: `Builder` / `RpcBundle` / `decode_field`),
`arbor-plugin-core` (`PluginHost` + the reflection types), `serde`, `serde_json`,
`semver`.

## Consumed by

The product backends (`corvus-be`; future `merula-be` / `sitta-be`).
