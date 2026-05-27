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

Today these all reach into `src-tauri/src/plugin/runtime/manifest/*`. After
the split they all reach into `arbor-plugin-types` — a leaf crate with no
behaviour.

## Contents (planned)

- `Manifest` — full `plugin.toml` shape (name, version, author,
  description, `[permissions]`, `[scheduler]`, `[[dependencies]]`,
  `[fs] scope`, `experimental`, `min_arbor_version`, `category`,
  `keywords`, `icon`, `doc_file`, `homepage`, `repository`).
- `Permissions` — typed `[permissions]` block. Stays serde-compatible
  with the existing JSON-on-wire shape the Plugin Manager already speaks.
- `Dependency` — `{ name, version (semver constraint), optional }`.
- `Schedule` + `ScheduleTrigger` — `fixed_rate` / `fixed_delay` / `cron`
  + `initial_delay_sec` + `only_when_focused` + `on_load`.
- `HookCatalog` — hook **names** (constants like `HOOK_ON_PRE_COMMIT`) and
  their context-schema descriptors. **Not** the dispatcher — that's in
  `arbor-plugin-api`.
- `PluginConfig` — the host-side per-plugin settings struct (enabled
  state, runtime overrides). Part of strada (2) for config — this is the
  fragment `arbor` aggregates into the global `AppConfig`.

## Depends on

- `arbor-core` — for `AppError` reuse and the few common shapes.

External: `serde`, `serde_json`, `thiserror`, `toml`, `semver`.

## Consumed by

- `arbor-plugin-api` — the hook dispatcher reads the hook catalog.
- `arbor-plugin-marketplace` — manifest parsing for catalog and install.
- `arbor-plugin-core` — runtime loads `Manifest`, applies `Permissions`,
  registers `Schedule` entries with `arbor-scheduler`.
- `arbor` (Tauri shell) — Plugin Manager UI commands serialize/deserialize
  these shapes over IPC.

## Notes

- `plugin.toml` is contract. The hard rule from CLAUDE.md applies: do not
  add fields without explicit user approval, even speculative ones.
- The hook catalog kept here is canonical — when adding/renaming/removing
  a hook, update this catalog AND the SDK `sdk.d.lua` in the
  `arbor-extensions` repo in the same change.
