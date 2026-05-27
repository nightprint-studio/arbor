# arbor-plugin-api

Runtime-agnostic plugin extension API. No `mlua`, no Tauri — just the types
every Arbor domain crate (git-provider, issue-tracker, pipeline, …) uses to
contribute namespaces of functions, hook definitions, and permission keys
to whatever scripting runtime the host wires up.

## Purpose

`src-tauri` used to be the only place that knew about plugin-facing
surface — it owned `arbor.fs`, `arbor.repo`, `arbor.terminal`, plus every
hook, plus every permission. Once we split git-provider, issue-tracker,
pipeline et al. into their own crates, *they* are the ones that should
declare `arbor.gitprovider.create_mr`, `on_mr_created`, the `gitprovider`
permission. But none of them want to depend on the Lua runtime.

This crate is the contract that lets that happen:

- A [`NamespaceContributor`] is the unit a crate ships — "here are my
  functions, my hooks, my permission keys".
- A [`PluginRegistry`] is what the host populates at boot by calling
  `contribute(&mut reg)` on every contributor.
- A [`HookDispatcher`] is the broker the runtime adapter
  (`arbor-plugin-core::LuaRuntime`, tomorrow `WasmRuntime`) registers a
  [`HookListener`] against, so a single `dispatcher.fire(...)` fans out to
  every runtime without any of them knowing the others exist.

See [`docs/plugin-api-architecture.md`](../../../docs/plugin-api-architecture.md)
for the full design — decisions D1–D8, the migration roadmap (PRs #3–#6+),
and a worked example.

## Contents

- [`value`](src/value.rs) — [`PluginValue`] enum (the in-process bridging
  value type — D1), [`PluginMapExt`] for ergonomic key access on `Map`
  payloads, JSON round-trip via [`PluginValue::from_serializable`] /
  `from_json` / `to_json`.
- [`error`](src/error.rs) — [`PluginError`] (`thiserror`) with constructors
  (`bad_args`, `not_found`, `domain`, `other`).
- [`perm`](src/perm.rs) — [`PermSchema`] (Bool / String / StringList /
  ordered Enum), [`PermReq`] (Has / AtLeast / Equals), [`PermissionDef`],
  [`PermissionsView`] trait, [`ManifestPermError`].
- [`hook`](src/hook.rs) — [`HookDef`] (dynamic — with [`HookKind`] flag)
  reusing `HookField` / `FieldType` from `arbor-plugin-types`.
- [`ctx`](src/ctx.rs) — [`PluginCtx`] trait: plugin name, manifest,
  permission lookup, app-event emitter. Sync methods so it stays object-safe.
- [`func`](src/func.rs) — [`PluginFn`] (`async_trait`) + [`NamespaceFn`]
  entry: namespace + name + requires + body.
- [`namespace`](src/namespace.rs) — [`NamespaceContributor`] trait.
- [`registry`](src/registry.rs) — [`PluginRegistry`] with `register_fn` /
  `register_hook` / `register_permission` / `lookup_fn` / `iter_*` /
  `validate_manifest` (no-op for now, lights up in PR #4) and the async
  [`PluginRegistry::invoke`] gate that checks `requires` before calling
  the body.
- [`dispatcher`](src/dispatcher.rs) — [`HookDispatcher`] + [`HookListener`]
  (`async_trait`) with `fire` / `fire_vetoable` (D7).

## Depends on

- `arbor-plugin-types` — shared atoms reused without duplication:
  `Manifest`, `HookField`, `FieldType`.

External: `serde`, `serde_json`, `thiserror`, `async-trait`, `toml`.

Intentionally not a dependency: `arbor-core` (no `AppCtx` / `CoreError`
boundaries cross here), `mlua` (this crate is runtime-agnostic), `tauri`
(host shell concern).

## Consumed by

- `arbor-plugin-core` *(planned, PR #4)* — instantiates `LuaRuntime` from a
  populated `PluginRegistry` + `HookDispatcher`. Migrates the existing
  `src-tauri/src/plugin/api/ns/*` namespaces to [`NamespaceContributor`]s
  and replaces `hook_registry.rs` with a [`HookListener`] impl.
- `arbor-git-provider-api`, `arbor-issue-tracker-api`,
  `arbor-pipeline-api`, … *(planned, PR #6+)* — each ships its own
  [`NamespaceContributor`] for the domain's plugin surface.

## Public API: the prelude

Workspace convention — every Arbor library crate exposes its public surface
through a `prelude` module:

```rust
use arbor_plugin_api::prelude::*;
// …or, when only one or two types are touched:
use arbor_plugin_api::prelude::{PluginRegistry, NamespaceFn};
```

The per-feature submodules (`value`, `registry`, …) stay `pub` for
rustdoc navigation, but call sites should reach for the prelude.

The prelude re-exports `FieldType`, `HookField`, and `Manifest` from
`arbor-plugin-types` for ergonomy — a contributor doesn't need a second
`use` line just to spell its hook context fields. `HookDef` from
`arbor-plugin-types` is deliberately **not** re-exported: there are two
`HookDef`s in the workspace (the static catalog one in plugin-types and
the dynamic one with [`HookKind`] in this crate), and the explicit
`arbor_plugin_types::prelude::HookDef` path on the rare static-catalog
call site keeps the distinction loud.

## Notes

- The registry's `validate_manifest` is a no-op today. It lights up in
  PR #4 once `Permissions` grows the `ext: HashMap<String, toml::Value>`
  field that holds crate-contributed permission keys.
- `PluginRegistry::register_*` panic on duplicate keys on purpose — this
  is a boot-time programmer error, not a recoverable runtime condition.
- `HookDispatcher::fire` walks listeners sequentially. The typical
  listener fans out into a single-threaded VM (mlua) so concurrent firing
  buys nothing and obscures ordering.
- The dispatcher is **name-agnostic**: if you ever find yourself adding a
  `match name { "on_xyz" => … }` here, that knowledge belongs in the
  domain crate that owns the hook, not in plugin-api.
