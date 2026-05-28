# Plugin UI: dispatch union, command invocation & event/patch protocol

> Status: **design** (not yet implemented). Canonical plan for two
> additive evolutions of the plugin UI surface. Keep in sync as steps land.
>
> **Audience**: contributors working on the plugin system internals
> (`arbor-plugin-core`, `src-tauri` shell, `FormNodeRenderer` + friends).

## Why

Two capabilities are missing from the declarative plugin UI, and they share
one seam:

1. **Command invocation** — an actionable node can only call back into its
   *own* plugin's Lua handler (`action: string` → `fire_plugin_action`).
   There is no way to invoke a host Tauri command (commit, push, open
   settings…) or another plugin's command declaratively.
2. **Event/patch protocol + granular state** — every action serializes the
   *entire* form (`{...values, state}`) and round-trips over Tauri events;
   updates back are whole-field (`set_value`) or whole-tree (`replace`).
   This is fine for forms but is the wall for studio-grade, stateful,
   high-frequency UIs (editor, tree, diff).

The unifying insight: both are **purely additive** and meet at a single
abstraction — a **dispatch target union** on every actionable node slot.
Nothing existing changes meaning; old plugins keep working untouched.

## Goals / non-goals

**Goals**
- A node slot can target either the owning plugin (today) or a registered
  command (new), via one union.
- Command invocation is capability-gated, reusing the permissions the
  plugin already requested — no new per-command allowlist in the manifest.
- A low-latency channel: scoped per-node events + granular patches, opt-in,
  living next to the existing whole-form model.
- Zero regressions: every existing plugin and the 5 built-in studios run
  unchanged.

**Non-goals (this doc)**
- The new host widgets themselves (editor/diff/tree) — they are *consumers*
  of this protocol, specified separately. Briefly sketched in §5.
- WASM runtime adapter — orthogonal; this protocol is what makes a WASM
  backend useful to a rich UI, but the adapter is its own work.
- Replacing the whole-form model. It stays the default.

---

## 1. The dispatch union (the shared seam)

Every actionable slot on every node (button `action`, select `change`
action, future `onSelect`/`onEdit`/…) resolves to a **dispatch target**:

```ts
type DispatchTarget =
  | { kind: 'action';  name: string }                 // callback to owning plugin (today)
  | { kind: 'command'; id: string; args?: unknown }   // §2 — registered command
  // future: { kind: 'service'; plugin: string; service: string; args?: unknown }
```

- **`action: string` becomes sugar** for `{ kind: 'action', name }`.
  Identical semantics and identical legacy payload (`{...values, state}`)
  on this path. No behavior change.
- One FE helper routes everything:
  ```ts
  function dispatch(target: DispatchTarget, payload: unknown): Promise<void>
  // kind:'action'  → firePluginAction(pluginName, target.name, json(payload))
  // kind:'command' → fireCommand(target.id, target.args, json(payload))
  ```
- Both legacy button actions *and* the new per-node events go through this
  helper. One place to extend for `service` later.

**Files**: `src/lib/types/plugin.ts` (`DispatchTarget` type),
`form-nodes/dispatch.ts` (`toDispatchTarget` desugar helper),
`FormNodeRenderer.svelte` (the `dispatch` executor + routing).

**Landed (step 1)**: desugaring is done at the dispatch point via
`toDispatchTarget(slot)` — not by rewriting the node tree in `normalizeNode`.
Rewriting `action` into a `dispatch` object inside `normalizeNode` would force
every sub-renderer that reads `.action` to change (or duplicate state and risk
drift). Instead the node tree is untouched and the legacy `action: string` is
desugared to `{kind:'action',name}` only when a slot fires. `handleButtonAction`
is the single routing point today; direct `firePluginAction` callsites
(autocomplete fetch, vec_field axis, field leaf commit) stay as-is per §3.3.

**Regression note**: a node with `action: "foo"` produces
`{kind:'action',name:'foo'}` at dispatch time; the wire payload to the plugin
is byte-for-byte the same.

---

## 2. Command invocation (capability-gated)

### Model (per product decision)

- The invocable units are **host Tauri commands** (plus plugin-contributed
  commands). The plugin does **not** enumerate specific command ids in its
  manifest.
- The manifest gains a single **command-invocation permission** (the plugin
  declares "I want to invoke commands at all").
- **Per-command authorization derives from existing permission tiers.**
  Each invocable command is mapped on the BE to the permission it requires
  (e.g. `git read`, `git write`, `fs write`, `provider write`…). A plugin
  may invoke a command only if it already holds that command's required
  permission.
- The BE owns a **command registry / mapping of every invocable command**:
  an allowlist defined on the main (host built-ins, each tagged with its
  required tier) **plus** commands contributed by plugins.

### Manifest

```toml
[permissions]
# existing: git, fs, terminal, provider, issues, network, …
command_invoke = true   # NEW — opt-in to invoking registered commands
```

Default `false` → no plugin gains command access by accident. (Naming:
`command_invoke` proposed; confirm in §7.)

`PermissionDef` / `manifest.rs` (`crates/plugin/types/src/permissions.rs`,
`manifest.rs`) gain the flag. Default keeps the struct backward-compatible.

### BE command registry

```rust
// new: src-tauri (shell) — host owns the canonical mapping
pub struct InvocableCommand {
    pub id:        String,           // "arbor:git.commit", "<plugin>::run"
    pub required:  RequiredPerm,     // derived from existing tiers
    pub kind:      CommandKind,      // HostBuiltin | Plugin { owner }
}

pub enum RequiredPerm {
    None,
    Git(GitLevel),          // reuse existing tiers
    Fs(AccessLevel),
    Provider(AccessLevel),
    Terminal(TerminalLevel),
    // …
}

pub struct CommandRegistry {
    by_id: HashMap<String, InvocableCommand>,
}
```

- **Main allowlist**: host registers built-ins it deems plugin-invocable,
  each tagged with `required`. Commands *not* in the registry are not
  invocable from plugins (closed by default — destructive/internal commands
  simply aren't registered).
- **Plugin-contributed**: `arbor.command.register` is *extended* (optional
  fields) so a registered command can be marked invocable + carry a
  `required` hint. Existing `register` calls (palette-only) keep working.

### New Tauri command

```rust
#[tauri::command]
async fn fire_command(
    caller_plugin: String,   // who is invoking (the UI's owning plugin)
    id: String,
    args: serde_json::Value,
    context_json: String,    // the node payload (scoped or full)
) -> Result<(), AppError>
```

Enforcement (both gates):
1. caller has `command_invoke` in its manifest, else `AppError` (clean,
   not a panic);
2. caller holds the permission `registry.get(id).required` demands, checked
   with the **same** logic as a user-initiated call.

Then dispatch to the command's handler (host fn, or `fire_plugin_action`
on the owner for plugin-contributed commands).

### Lua symmetry

Add `arbor.command.fire(id, ctx)` → emits to the same `fire_command` path
(so node dispatch and runtime invocation share one route). New, additive.
`arbor.command.register`/`unregister` unchanged except optional new fields.

### Landed (step 2a)

Scope decision: **plugin-contributed commands only**; host built-ins deferred
to 2b (they need extraction from the async Tauri command wrappers).

- **No parallel registry struct.** Invocable plugin commands live in the
  existing `ContributionRegistry` under `COMMAND_PALETTE`. `CommandPayload`
  gained `invocable: bool` (default false → closed) + `required: RequiredPerm`.
  A dedicated `CommandRegistry` for host built-ins lands with 2b.
- **One resolver, two entry points.** `PluginHost::invoke_command(caller, id,
  ctx)` owns resolution + both gates; `fire_command` (Tauri) and
  `arbor.command.fire` (Lua, gated on `command_invoke`) both call it.
- **Id scheme.** Plugin commands: `<owner>::<id>`. Host: `arbor:area.verb`
  (rejected with `host_unavailable` until 2b).
- **Gating.** Caller needs `command_invoke` **and** must `satisfy` the
  command's `required` tier (reuses the existing `>=` tier comparison).
- **Cross-plugin model.** No `service_call`-style flag on the caller — gated
  purely on `command_invoke` + the target's `required` perm (per product
  decision).
- **FE surface.** `button.dispatch?: DispatchTarget` routes a button to a
  command; `dispatch()` in `FormNodeRenderer` gained the `command` case.
  Other slots (menu options, header actions, …) still use legacy `action`
  strings — they can adopt `dispatch` incrementally.

### Regression safety (§2)

Everything new: a manifest flag (default off), a BE registry, a new Tauri
command, a new `dispatch.kind`, an additive Lua fn. `fire_plugin_action`,
`arbor.command.register`'s existing shape, and the palette flow are
untouched. A node without `kind:'command'` never reaches any of it.

---

## 3. Event/patch protocol + granular state

All additions are **opt-in and coexist** with the whole-form model. The
existing channel (`plugin:form` open; `plugin:form-update` ops
set_value/set_options/set_disabled/replace/set_loading/close;
`plugin:autocomplete-options`) stays exactly as is.

### 3.1 Scoped per-node events

Today only buttons/selects fire, and they ship `{...values, state, extra}`
(`FormNodeRenderer.buildActionPayload`). New, high-frequency slots
(`onSelect`, `onEdit`, `onScrollRange`, …) emit a **scoped** payload — just
that node's value/id plus an optionally declared slice — instead of the
whole form. Each slot is a `DispatchTarget` (so it can also target a
command).

- Legacy buttons keep shipping the full payload. **No change** to the
  existing ctx shape plugins already read.
- Scope is a property of the *slot*, independent of dispatch kind.

### 3.2 Granular patches (new `plugin:form-update` ops)

Two new ops in the existing switch — existing ops untouched:

```
{ op: "patch", ops: JsonPointerOp[] }      // mutate node tree in place
{ op: "set_state_path", path, value }      // mutate a slice of liveState
```

- `patch` applies JSON-pointer-style `replace`/`add`/`remove` against the
  **node tree** (addressed by stable node `id`/path) without re-mounting.
  Sibling to `replace`, which stays for whole-tree swaps.
- `set_state_path` updates one slice of `liveState` instead of replacing
  the whole blob (today only `replace { state }` can touch it).
- **Addressability**: patches require stable ids. Nodes already auto-id in
  `normalizeNode`; for patch targets the plugin supplies stable ids. A node
  without a stable id simply can't be patched (it can still be `replace`d).

`FormNodeRenderer`'s `plugin:form-update` listener (already a switch) gains
two cases; the patch applier mutates `nodes`/`values`/`liveState` `$state`
in place.

### 3.3 Concurrency

- Legacy `action` path keeps single-flight `actionPending` → **no behavior
  change** for existing buttons. (Separately worth fixing the silent drop,
  but that's out of scope here — flagged in §7.)
- The new scoped-event path uses **per-node in-flight tracking** (keyed by
  node id + slot), so concurrent interactions on different nodes don't block
  each other and a high-frequency widget isn't gated by a global lock.

### 3.4 Latency for hot widgets

Truly hot interactions (editor keystrokes, scroll) **never** round-trip
per event: the host widget owns local state and emits only *semantic*
events (value committed, selection changed), debounced. That is a property
of the widget (§5), not of the protocol — the protocol only needs scoped
events + patches to support it.

### Regression safety (§3)

New event names/slots are opt-in; new ops are new switch cases; granular
state is additive; legacy concurrency unchanged. Existing forms render and
behave identically.

---

## 4. Host widgets (future consumers — sketch only)

Built on §1–3, exposed as FormNode types (model: data → native Svelte
widget). Specified separately:

- **`editor`** — CodeMirror-backed, editable (today `code` is read-only);
  emits `onEdit` (debounced, scoped) + `onSelect`.
- **`diff`** — text + tree diff viewer.
- **`data_tree`** — lazy children fetch (via a dispatch slot), inline edit,
  selection events, virtualization for large docs.
- **workspace full-view container** — a main-area surface beyond modal /
  sidebar.

These are what let a plugin rebuild a studio entirely declaratively, and
ultimately retire `StudioModal.svelte`.

---

## 5. End-to-end regression strategy

- **Additive-only contract**: no existing field changes meaning; `action:
  string` desugars; legacy payloads byte-identical.
- **New seams**: dispatch union, `fire_command`, `command_invoke`, ops
  `patch`/`set_state_path`, scoped slots — all reachable only by new code.
- **Closed-by-default capability**: `command_invoke` off; command registry
  is an allowlist; per-command perm reuses existing tiers.
- **Concurrency split**: legacy single-flight retained; new path per-node.
- **Migration**: 5 built-in studios + all installed plugins unchanged.

## 6. Phasing (each step independently shippable & verifiable)

1. ✅ **Dispatch union + desugar `action`** — pure refactor, zero behavior
   change. Establishes the shared seam. *(landed — see §1 "Landed (step 1)")*
2. **Command invocation (Feature A)** — manifest `command_invoke`, BE
   command registry + perm mapping, `fire_command`, `kind:'command'`,
   `arbor.command.fire`. Self-contained.
   - ✅ **2a (landed)** — plugin-contributed commands only. See "Landed
     (step 2a)" below.
   - ⏳ **2b** — host built-in allowlist (`arbor:area.verb`). Deferred: needs
     host command logic extracted from the async Tauri wrappers into a
     callable registry. `fire_command` currently rejects `arbor:*` ids with
     `host_unavailable`.
3. **Patch ops** (`patch`, `set_state_path`) — additive update ops.
4. **Scoped per-node events + per-node concurrency.**
5. **Host widgets** (editor/diff/tree/full-view) consuming §1–4.

## 7. Open questions

- Manifest flag name: `command_invoke` vs `commands` (bool) vs
  `invoke_commands`. (Decision: single invocation permission, not a
  per-command list.)
- Command id scheme: `arbor:git.commit` (namespaced host) and
  `<plugin>::<id>` (plugin) — confirm separators.
- `RequiredPerm` granularity: reuse existing tier enums verbatim, or a
  flattened perm key per command?
- Cross-plugin command invocation: gate purely on the target's `required`
  perm, or also require a `service_call`-style flag on the caller?
- Should the legacy `action` single-flight silent-drop be fixed in the same
  effort (queue / disable / toast) or tracked separately?
- Patch addressing: JSON-pointer over the node tree by id, vs a flat
  `id → partial-node` map. Tradeoff: expressiveness vs simplicity.

## 8. Docs / SDK to update when implementing

- `sdk.d.lua` (arbor-extensions repo): `arbor.command.fire`, extended
  `arbor.command.register`, dispatch union on node slots, new node event
  slots, `command_invoke` permission.
- In-app docs (`PluginDevApiUI.svelte`, `PluginDevHooks.svelte`,
  permission docs): command invocation, dispatch, event/patch.
- `hook_catalog.rs`: unaffected (no new hooks) — note in PR.
- `CHANGELOG.md` `[Unreleased]`: user-facing additions per step.
