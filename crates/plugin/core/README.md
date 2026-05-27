# arbor-plugin-core

The plugin runtime host: mlua VM management, lifecycle, sandbox, and the
built-in `arbor.*` Lua API surface.

## Purpose

This is the biggest crate of the refactor — the actual machinery that
loads, runs, and tears down user-authored Lua plugins. It owns:

- one `mlua::Lua` per loaded plugin,
- the lifecycle hooks (`on_plugin_load` / `on_plugin_unload`),
- the entire `arbor.*` namespace exposed to plugin code (notify, ui.form,
  ui.settings, fs, http, scheduler, service, brp, …),
- the sandbox (file-scope guards, network allow-list, capability checks
  against the manifest's `[permissions]`),
- the per-plugin `[scheduler]` registrations (delegated to
  `arbor-scheduler`),
- the `LuaListener` adapter that bridges Lua callbacks into the
  `arbor-plugin-api` dispatcher.

It's the largest crate because the `arbor.*` API surface alone is dozens
of namespaces. Each namespace stays as today (`api/ns/<name>.rs`); the
crate boundary just makes the consumer set explicit.

## Contents (planned)

- `host` — `PluginHost` struct: registry of loaded plugins, lifecycle
  methods (`load_all`, `enable`, `disable`, `reload`), dispatcher wiring
  to `arbor-plugin-api`.
- `loaded` — `LoadedPlugin` with the mlua state, manifest, schedule
  handles, service exports, settings store.
- `sandbox` — capability enforcement against `Permissions`. File access,
  network host allow-list, IPC scope.
- `lifecycle` — disk scan, topological sort by `[[dependencies]]`,
  load/unload/reload sequencing.
- `api/ns/*` — all Lua-facing namespaces (`notify`, `ui`, `fs`, `http`,
  `scheduler`, `service`, `brp`, `cloud`, `keyring`, `git`, `repo`,
  `ipc`, `markdown`, `pipeline`, `studio`, …).
- `lua_builtins` — Lua-side companion code shipped as embedded strings
  (`promise_bridge.lua`, `async_lib.lua`).
- `service_registry` — inter-plugin `arbor.service.export` / `.call`
  runtime table.
- `settings_store` — per-plugin user-settings persistence (file under
  `arbor_config_dir()`).

## Depends on

- `arbor-core` — paths, http, AppCtx, AppError.
- `arbor-plugin-types` — manifest, permissions, dependency, schedule,
  hook catalog constants.
- `arbor-plugin-api` — register `LuaListener` per hook the plugin
  subscribes to.
- `arbor-scheduler` — register per-plugin schedules.

External: `mlua` (Lua 5.4, vendored), `tokio`, `serde`, `serde_json`,
`reqwest`, `dirs`, `toml`, `semver`, `regex`, `tracing`, `thiserror`,
`async-trait`.

## Consumed by

- `arbor` (Tauri shell) — owns the singleton `PluginHost`, wires
  `plugin_*` Tauri commands.
- (none else; the rest of the system reaches plugins via the dispatcher,
  not directly)

## Notes

- The `arbor.*` Lua API surface is contract for plugin authors but the
  user is the only consumer right now — breaking changes are allowed
  but each one updates `sdk.d.lua` (in the `arbor-extensions` repo) and
  the `PluginDevelopment.svelte` docs in the same change. Same rule as
  today.
- `mlua` lives ONLY here. If any sibling crate ends up needing it,
  that's a design smell — the bridge should pass through `Action` /
  `LuaListener` instead.
- The Studio plugins (json, yaml, ron, toml, properties) currently live
  in `src-tauri/` as Rust modules. They're earmarked for WASM-plugin
  migration; until then they stay in the `arbor` shell, NOT here.
