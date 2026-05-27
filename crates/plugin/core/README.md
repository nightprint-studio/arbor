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

## Status

PR #4 lands in incremental sessions — see
[`docs/plugin-core-architecture.md`](../../../docs/plugin-core-architecture.md)
for the tracker. Currently landed:

- ✅ Step 0 — crate scaffold.
- ✅ Step 1 — `AppCtx::record_plugin_log` (no-op default,
  `TauriAppCtx` delegates to `plugin_logs::record`).
- ✅ Step 2 — `Permissions.ext: HashMap<String, toml::Value>` +
  `PluginRegistry::validate_manifest` walks the catch-all against
  registered `PermissionDef` schemas.
- ✅ Step 3 — cross-plugin primitives migrated: `contribution`,
  `tree`, `toolchain` (state), `settings_store`, `event_bus`,
  `lua_ctx`. All now consume `Arc<dyn AppCtx>` instead of
  `tauri::AppHandle`. `ContributionRegistry::notify_changed` /
  `notify_containers_changed` dropped their per-call handle argument
  in favour of a `install_app_ctx` boot-time hook. src-tauri keeps
  shim `pub use` re-exports until the final cleanup step.

## Contents

Modules already migrated (call sites go through
`arbor_plugin_core::prelude::*`):

- `contribution` — `ContributionRegistry`, contribution points and
  payload schemas, container definitions, coalesced
  `arbor://contributions-changed` emit.
- `tree` — `TreeStore` + `IconRegistry` for `kind="tree"` plugin
  sidebars.
- `toolchain` — host-side `ToolchainRegistry` (per-kind JSON store
  under `arbor_config_path("toolchains")`).
- `settings_store` — JSON read/write helpers for per-plugin global /
  project settings.
- `event_bus` — namespaced frontend emit wrapper
  (`plugin:<name>:<event>`).
- `lua_ctx` — per-VM `PluginLuaCtx` stashed in `lua.app_data`; the
  bridge that lets `&Lua`-only code paths route runtime errors to
  the Plugin Logs panel.

Still in the src-tauri shell, scheduled for later sessions:

- `host` — `PluginHost` struct: registry of loaded plugins, lifecycle
  methods (`load_all`, `enable`, `disable`, `reload`), dispatcher
  wiring to `arbor-plugin-api`.
- `loaded` — `LoadedPlugin` with the mlua state, manifest, schedule
  handles, service exports, settings store.
- `sandbox` — capability enforcement against `Permissions`. File
  access, network host allow-list, IPC scope.
- `lifecycle` — disk scan, topological sort by `[[dependencies]]`,
  load/unload/reload sequencing.
- `api/ns/*` — all Lua-facing namespaces (`notify`, `ui`, `fs`,
  `http`, `scheduler`, `service`, `brp`, `cloud`, `keyring`, `git`,
  `repo`, `ipc`, `markdown`, `pipeline`, `studio`, …). The
  "host-pure" subset migrates into this crate; src-tauri-coupled
  namespaces stay in the shell as `LuaNamespaceInstaller`
  implementations.
- `lua_builtins` — Lua-side companion code shipped as embedded
  strings (`promise_bridge.lua`, `async_lib.lua`).
- `service_registry` — inter-plugin `arbor.service.export` / `.call`
  runtime table.

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
