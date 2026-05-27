# arbor-plugin-types

Pure data shapes shared by the plugin runtime and the marketplace. No
runtime behaviour, no mlua, no Tauri — just types.

## Purpose

Several places in the codebase need to know the *shape* of plugin metadata
without dragging in the plugin runtime:

- the marketplace reads `plugin.toml` from a third-party repo to build a
  catalog entry,
- the installer validates a downloaded ZIP's manifest before placing it on
  disk,
- the plugin runtime parses `plugin.toml` on load to wire permissions and
  schedule registrations,
- the in-app docs panel renders the hook catalog.

Everything below lives here as a pure-data leaf crate; behaviour (disk
walking, dependency-graph topo sort, `plugin_states.json` round-tripping,
Lua sandboxing, scheduler dispatch) stays in the host crate (`arbor` /
`arbor-plugin-core`).

## Contents

- [`manifest`](src/manifest.rs) — [`Manifest`] (the `plugin.toml` shape),
  [`ManifestParseError`], [`ManifestParseFailure`], and the pure-parsing
  entry point `Manifest::from_toml_str`.
- [`permissions`](src/permissions.rs) — [`Permissions`] + the typed access
  tiers ([`AccessLevel`], [`GitLevel`], [`TerminalLevel`], [`EnvReadPerm`]).
- [`dependency`](src/dependency.rs) — [`Dependency`] (plugin-to-plugin
  requirement, semver string + optional flag) and [`LoadFailure`] (record
  of a plugin that didn't satisfy its dependency graph at load time).
- [`schedule`](src/schedule.rs) — [`SchedulerSection`] (manifest opt-in),
  [`Schedule`] / [`ScheduleTrigger`] (Lua-registered schedule shape),
  [`ScheduleStatus`] (UI-side runtime status), the [`ScheduleRegistry`]
  type alias the host populates, and [`parse_duration_secs`].
- [`hooks`](src/hooks.rs) — [`Hooks`] (per-hook opt-in flags read from
  `plugin.toml`'s `[hooks]` block, with `subscribes_to(name)` lookup).
- [`sandbox`](src/sandbox.rs) — [`Sandbox`] (the `[sandbox]` Lua-stdlib
  allowlist).
- [`hook_catalog`](src/hook_catalog.rs) — canonical [`HOOK_CATALOG`] +
  [`HookDef`] / [`HookField`] / [`FieldType`] + `find(name)`. **Not** the
  dispatcher — that's in `arbor-plugin-api`.

`PluginConfig` is listed in the workspace blueprint but not yet
introduced — strada (2) of the per-domain config split. It will land
alongside `arbor-plugin-core` (or earlier, if another consumer needs it).

## Depends on

External: `serde`, `thiserror`, `toml`.

`arbor-core` is not currently a dependency — none of the migrated types
need `CoreError`, paths, or HTTP helpers. It will be added back when (and
only when) the first type here genuinely references it (e.g. once
`PluginConfig` introduces the `AppCtx` trait at its boundaries).

## Consumed by

- `arbor-plugin-api` *(planned)* — the hook dispatcher reads the hook
  catalog.
- `arbor-plugin-marketplace` *(planned)* — manifest parsing for catalog
  and install.
- `arbor-plugin-core` *(planned)* — runtime loads [`Manifest`], applies
  [`Permissions`], registers [`Schedule`] entries with `arbor-scheduler`.
- `arbor` (the Tauri shell) — Plugin Manager UI commands serialise /
  deserialise these shapes over IPC.

## Public API: the prelude

Workspace convention — every Arbor library crate exposes its public surface
through a `prelude` module:

```rust
use arbor_plugin_types::prelude::{Manifest, Permissions, Hooks};
// …or, when a module touches many of the types:
use arbor_plugin_types::prelude::*;
```

The per-feature submodules (`manifest`, `permissions`, …) stay `pub` for
discoverability in rustdoc, but call sites should reach for the prelude.

## Notes

- `plugin.toml` is contract. The hard rule from CLAUDE.md applies: do not
  add fields without explicit user approval, even speculative ones.
- The hook catalog kept here is canonical — when adding/renaming/removing
  a hook, update this catalog AND the SDK `sdk.d.lua` in the
  `arbor-extensions` repo in the same change.
