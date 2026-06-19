# arbor-plugin-core

The plugin runtime host: mlua VM management, lifecycle, sandbox, hook
routing, and the host-pure slice of the built-in `arbor.*` Lua API surface.

## Purpose

The actual machinery that loads, runs, and tears down user-authored Lua
plugins. It owns:

- one `mlua::Lua` per loaded plugin,
- the lifecycle hooks (`on_plugin_load` / `on_plugin_unload`),
- the host-pure slice of the `arbor.*` namespace (notify, fs, http,
  settings, ui.*, scheduler, service, timer, studios, …),
- the sandbox (file-scope guards, network allow-list, capability checks
  against the manifest's `[permissions]`),
- the per-plugin `[scheduler]` registrations (delegated to
  `arbor-scheduler`),
- the `LuaHookListener` adapter that bridges Lua callbacks into the
  `arbor-plugin-api` `HookDispatcher`.

Namespaces that need shell-internal concepts (`git::*`, `pipeline::*`,
`jobs::*`, `terminal::*`, `workspace::*`, `brp::*`, `cloud::*`, …) stay in
the Tauri shell crate as [`LuaNamespaceInstaller`] implementations and are
wired into the runtime at boot. They migrate into their own domain crates
in PR #6+.

## Status

✅ **PR #4 landed.** The crate is the canonical home of the plugin runtime;
the Tauri shell keeps only the domain-coupled namespace installers and the
adapters that wire them in. See
[`docs/plugin-core-architecture.md`](../../../docs/plugin-core-architecture.md)
for the full migration history.

Public API is reached exclusively through
[`prelude`](src/prelude.rs) — `use arbor_plugin_core::prelude::*;` or a
fully-qualified `arbor_plugin_core::prelude::PluginHost`. The per-feature
submodules stay `pub` for rustdoc navigation, but call sites go through the
prelude.

## Contents

- `contribution` — `ContributionRegistry`, contribution points and
  payload schemas, container definitions, coalesced
  `arbor://contributions-changed` emit.
- `tree` — `TreeStore` + `IconRegistry` for `kind="tree"` plugin
  sidebars.
- `toolchain` — host-side `ToolchainRegistry` (per-kind JSON store
  under `profile_plugins_dir()/toolchains`).
- `settings_store` — JSON read/write helpers for per-plugin global /
  project settings.
- `lua_ctx` — per-VM `PluginLuaCtx` stashed in `lua.app_data`; the
  bridge that lets `&Lua`-only code paths route runtime errors to
  the Plugin Logs panel.
- `sandbox` — per-plugin Lua VM construction. Loads the curated
  StdLib slice, hardens `os.*` per `permissions.env_read`, sandboxes
  `require()` to the plugin dir, injects the embedded `arbor.*`
  builtins, and delegates the `arbor.*` namespace surface itself to
  the host-supplied [`LuaApiInstaller`].
- `lua_builtins/` — eight embedded Lua modules shipped via
  `include_str!`: `schema`, `async_lib`, `event`, `promise_bridge`,
  `builders`, and the `core/{_util, edit, assert}` pipeline-op
  catalog.
- `runtime::consts` — `ARBOR_API_VERSION`, `ARBOR_APP_VERSION`,
  `current_os()`.
- `runtime::loaded` — `LoadedPlugin` / `DormantPlugin` +
  `TimerCancels` / `TimerCounter`.
- `runtime::manifest` — `plugin.toml` discovery + topological sort
  over `[[dependencies]]` + persisted enable/disable state
  (`profile_plugins_dir()/plugin_states.json`). Caller-supplied roots
  (`discover_in_roots`) so the marketplace overlay stays decoupled.
- `runtime::scheduler` — bridge between `PluginHost` and the shared
  `arbor-scheduler` engine.
- `runtime::host` — `PluginHost` struct + lifecycle (load/enable/
  disable/delete), service invocation, command invocation
  (`invoke_command` — capability-gated dispatch of `<owner>::<id>` plugin
  commands and `arbor:area.verb` host built-ins; the built-in allowlist +
  required tiers live in `host_command_required`, the handlers in the shell
  via `AppCtx::invoke_host_command`),
  pipeline-op invocation, dependency cascade preview, frontend-facing
  `list_plugin_info`.
  The `hooks` submodule holds the surviving subscription queries
  (`plugin_has_handler`, `remove_hook`) — firing moved to `hook_router`.
- `hook_router` — the Lua-side hook pipeline: low-level dispatch
  helpers (`fire`, `fire_collecting`, `matches_pattern`), the
  broadcast / targeted / vetoable free functions over `&PluginHost`
  (`fire_broadcast`, `fire_on`, `fire_vetoable`), and the
  `LuaHookListener` adapter that the runtime-agnostic
  `arbor_plugin_api::HookDispatcher` drives.
- `lua_api` — the `arbor.*` surface orchestrator:
  - `ctx` — `ApiCtx`, the per-`register()` capture bag.
  - `helpers/*` — pure helpers shared by every namespace installer
    (`convert`, `tuple`, `fs_perm`, `glob`, `http_worker`, `json_patch`,
    `settings_scope`, `timer`, `contrib_write`, `xml_patch`).
  - `ns/*` — the host-pure namespaces (`log`, `events`, `json`, `text`,
    `meta`, `notify`, `hooks`, `command`, `keybinding`, `service`,
    `timer`, `scheduler`, `contribution`, `fs`, `http`, `settings`,
    `ui/*`, and the studios `json`/`yaml`/`toml`/`ron`/`properties`).
  - `register(lua, params, extra_installers)` — builds the `arbor.*`
    table, runs the in-crate host-pure namespaces, then the host-supplied
    [`LuaNamespaceInstaller`] slice (the shell's domain-coupled ns).

## Stays in the Tauri shell (PR #6+)

`src-tauri/src/plugin/ns_shell/*` keeps the namespaces that still reach
shell-internal types: `repo`, `mr`, `ci`, `issues`, `notes`, `pipeline`,
`cloud`, `brp`, `security`, `toolchain` (the ns), `terminal`, `tabs`,
`workspace`, `linked_worktrees`, `job`, and `ui/branding`. Each is a
[`LuaNamespaceInstaller`] wrapper; `shell_installers()` hands them to
`register()` after the host-pure namespaces. As each grows its own domain
crate they drop out of that list, and once it's empty the shell-side
`plugin` module disappears.

## Depends on

- `arbor-core` — paths, http, AppCtx, AppError.
- `arbor-plugin-types` — manifest, permissions, dependency, schedule,
  hook catalog constants.
- `arbor-plugin-api` — `HookDispatcher` / `HookListener` the
  `LuaHookListener` plugs into.
- `arbor-scheduler` — register per-plugin schedules + cron validation.

External: `mlua` (Lua 5.4, vendored), `tokio`, `serde`, `serde_json`,
`reqwest`, `dirs`, `toml`, `semver`, `regex`, `tracing`, `thiserror`,
`async-trait`, `futures-executor`.

## Consumed by

- `arbor` (Tauri shell) — owns the singleton `PluginHost`, wires the
  `plugin_*` Tauri commands, and supplies the domain-coupled
  `LuaNamespaceInstaller`s.
- (none else; the rest of the system reaches plugins via the dispatcher,
  not directly)

## Notes

- The `arbor.*` Lua API surface is contract for plugin authors but the
  user is the only consumer right now — breaking changes are allowed
  but each one updates `sdk.d.lua` (in the `arbor-extensions` repo) and
  the `PluginDevelopment.svelte` docs in the same change.
- `mlua` lives ONLY here. If any sibling crate ends up needing it,
  that's a design smell — the bridge should pass through `Action` /
  `LuaHookListener` instead.
- The Studio plugins (json, yaml, ron, toml, properties) live here as
  host-pure namespaces. They're earmarked for WASM-plugin migration.
