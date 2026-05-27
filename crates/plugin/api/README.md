# arbor-plugin-api

Hook dispatcher and listener registry. Name-agnostic by design.

## Purpose

Hooks fire from many places: a commit completes, a branch is renamed, a
PR is opened, an issue is transitioned. Today the dispatcher lives inside
the plugin runtime and is tightly coupled to `mlua::Lua` and the
`PluginHost` state.

Splitting it out gives two wins:

1. **Domains can fire hooks without depending on the Lua runtime.**
   `arbor-issue-tracker-jira` can call `dispatcher.fire("on_issue_transitioned", ctx)`
   without ever knowing what a Lua VM is.
2. **No dependency cycles.** This crate doesn't know any hook names — it
   takes them as `&str`. Each domain owns its own hook name constants
   (`HOOK_ON_ISSUE_LINKED` in `arbor-issue-tracker-api`, etc.). The
   dispatcher only routes.

## Contents (planned)

- `HookDispatcher` — the runtime registry of listeners. Public API:
  `fire(name, ctx)` async, `register_listener(name, listener)`,
  `unregister_listener(id)`.
- `HookListener` trait — implemented by adapters. Two known impls today:
  - `LuaListener` — lives in `arbor-plugin-core`, dispatches into a
    plugin's mlua VM.
  - `RustListener` — for built-in observers (e.g. the docs panel
    refreshing its hook catalog).
- `HookContributor` trait — the mechanism by which domains declare their
  hook surface for the in-app catalog. `arbor` aggregates all
  contributors at startup to build the `Shortcuts`/`Hook catalog` page.
- **Vetoable hooks**: the dispatcher recognises a return-value convention
  (currently used by `on_pre_commit`). A non-empty string in the result
  aborts the action and surfaces the message to the user.

## Depends on

- `arbor-core` — `AppError`, async primitives.
- `arbor-plugin-types` — to read the hook catalog metadata for
  validation, NOT the hook names themselves (those come from other
  domains).

External: `serde`, `serde_json`, `tracing`, `thiserror`, `async-trait`.

## Consumed by

- `arbor-plugin-core` — registers a `LuaListener` per plugin per hook.
- `arbor-plugin-marketplace` — fires its own hooks (on plugin install,
  uninstall, refresh).
- `arbor-pipeline-core` — fires pipeline lifecycle hooks.
- Any future domain that wants to expose extension points.

## Notes

- The dispatcher MUST stay name-agnostic. If you find yourself adding
  `match name { "on_issue_linked" => ... }` here, stop — that knowledge
  belongs in the dominio that owns the hook.
- Vetoable hooks deserve a typed return surface, not the current
  "non-empty string aborts" string sentinel. Open question for the
  redesign: introduce `HookResult::Continue` / `HookResult::Veto(reason)`
  or keep the stringly-typed convention for SDK simplicity. Decide
  when implementing.
