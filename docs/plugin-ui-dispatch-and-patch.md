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

#### Landed (step 4)

Design decisions (closed):

- **Scoped payload — `{ node_id, slot, value, state? }`.** `state` is an
  *optionally declared slice*: a slot lists `scope_state: string[]` keys and
  only those ride along (default: no state). No whole-form blob on the
  high-frequency channel.
- **`set_state_path` segments format** already chosen in step 3 (array of
  segments); scoped slots reuse that mental model for `scope_state`.
- **Concurrency — latest-wins, no gating.** A per-`node_id+slot` in-flight
  counter (`scopedInflight`) exists only for spinner/state; the path never
  blocks, so a hot widget always emits its newest event and different nodes
  never contend. The legacy button path keeps its single global
  `actionPending` single-flight — unchanged.
- **Opt-in via `dispatch`.** A value slot enters the scoped path only when it
  carries a `DispatchTarget` object (`{kind:'action'|'command', …}`); a bare
  `action`/`actions.change` string keeps the exact legacy whole-form payload.

Implementation (FE-only — scoped events reuse the existing `dispatch()`
executor, so no new Tauri command / Rust):

- `FormNodeRenderer`: `handleScopedDispatch(nodeId, slot, target, value,
  {stateKeys})` + `buildScopedPayload` + `isScopedPending`, exposed on the
  rendering `ctx`. `selectChange(node, value)` routes string→legacy /
  object→scoped.
- `helpers.ts::wrapSelectChange` now takes a node-aware `onChange(value)`
  callback (the renderer owns the routing); the select call site passes the
  node.
- Retrofit (opt-in, legacy untouched): leaf `field` (`leafFire`), `vec_field`
  (`{axis,index,value}`), and `select` `actions.change`.
- Types: `dispatch?`/`scope_state?` on `FormFieldBase`; `actions.change` on
  `FormFieldSelect` widened to `string | DispatchTarget`.
- The host widgets that consume this end-to-end (editor/tree/diff, §4) land in
  step 5; until then the retrofit slots are the live exercise of the channel.

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

#### Landed (step 3)

Design decisions (closed from §7):

- **Patch addressing — id-keyed ops, not JSON-pointer.** `patch` carries
  `patches: FormPatchOp[]`, each op targeting a node by its stable `id` plus
  one verb: `merge` (shallow-merge props), `set` (deep assign at a path of
  segments *inside* the node, e.g. `["options",0,"label"]`), `append` (push a
  child into an array prop, `to` default `"children"`), `remove` (splice the
  node out — removing a *child* = target it by its own id). No global tree
  indices are exposed, so ids don't drift the way pointer paths would.
- **`set_state_path` path — array of segments**, e.g.
  `{ "filters", "branch" }`. No dotted-string parsing, unambiguous with keys
  that contain `.`/`/`, idiomatic for Lua tables.
- **Scope — node tree only.** `patch` never touches field `values` (use
  `set_value`) nor the opaque blob (use `set_state_path`). On `set_state_path`,
  a Lua `nil` value DELETES the key (emitted as `{ delete: true }`, since Lua
  has no JSON-null literal).

Implementation:

- FE applier `form-nodes/patch.ts::applyPatchOps(roots, ops)` mutates the
  `nodes` `$state` tree in place (locate-by-id walks every branching container;
  appended nodes are `normalizeNode`'d). After a patch the listener runs
  `seedNewNodeState()` — an **additive-only** reconcile that seeds `values` and
  the collapse/tab/wizard/kv maps for any subtree an `append` introduced,
  without disturbing existing live state.
- `set_state_path` mutates `liveState` in place (creating intermediate
  containers; `delete` drops the leaf).
- Type `FormPatchOp` in `types/plugin.ts`. Lua: `arbor.ui.form.patch` +
  `arbor.ui.form.set_state_path` in `ns/ui/form.rs`. The whole-form ops
  (`replace`/`set_value`/`set_options`/`set_disabled`/`set_loading`/`close`)
  are untouched.

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
- **`diff`** — read-only diff viewer (unified + split). *(landed — see
  "Landed (step 5 — `diff`)" below)*
- **`data_tree`** — lazy children fetch (via a dispatch slot), inline edit,
  selection events, virtualization for large docs. *(landed as the `tree`
  node's dynamic mode — see "Landed (step 5 — `data_tree`)" below)*
- **workspace full-view container** — a main-area surface beyond modal /
  sidebar.

These are what let a plugin rebuild a studio entirely declaratively, and
ultimately retire `StudioModal.svelte`.

### Landed (step 5 — `editor`)

The first host widget. **Design decisions (closed):**

- **Library — reuse the existing CodeMirror 6.** No new dependency: the
  `editor` node wraps the existing `shared/studio/StudioTextPane.svelte` (the
  same controlled CM6 host the Studio modals use), so syntax highlighting,
  line numbers, search, history and the arbor-themed editor come for free.
- **Value model — value-bearing + scoped.** The node carries a `name`, so its
  document is collected into `values[name]` and submitted like any field, and
  the host can push new content with the existing `set_value` op (the pane is
  controlled, so an external write reconciles the buffer without an echo).
  *On top of that* it is the first live consumer of the §3.1 scoped channel.
- **Slots.** `on_edit` (debounced in the widget per §3.4 — `debounce_ms`
  default 300; slot `edit`, value = full text) and `on_select` (slot `select`,
  value = `{ from, to, text }`). Both accept a legacy action string or a
  `DispatchTarget`, route through `ctx.handleScopedDispatch`, and honour
  `scope_state`.
- **Languages.** Mapped to the studio set (`json`/`toml`/`yaml`/`ron`/
  `properties`/`plain`); unknown ids fall back to `plain`. Extending the
  grammar set is deferred (would need new parsers — ask before adding libs).

**Implementation (FE-only — no Rust/Tauri change):**

- `form-nodes/FormNodeEditor.svelte` (new) wraps `StudioTextPane`, renders the
  standard `.pf-field` chrome (label / validation error / hint / pill), binds
  the document to `ctx.values[name]`, and fires the two scoped slots.
- `StudioTextPane.svelte` gained one additive optional `onselect` callback
  (fires on a pure selection change — `selectionSet && !docChanged`).
- `FormNodeRenderer` routes `type:'editor'` → `FormNodeEditor` (before the
  `FormNodeField` catch-all).
- Type `FormFieldEditor` in `types/plugin.ts` (added to `FormFieldNode`); the
  node is value-bearing so `collectFields` seeds it automatically.

### Landed (step 5 — `diff`)

The second host widget. **Design decisions (closed):**

- **Renderer — reuse, don't rebuild.** The `diff` node wraps the app's own diff
  row renderers (`components/diff/DiffHunk.svelte` + the virtualized
  `VirtualHunk.svelte`), which are already self-contained (no `diffStore`
  dependency) and render read-only when `stageable` is false. A lean
  `FormNodeDiff.svelte` mounts them directly — deliberately *not* `DiffViewer`,
  which drags in `diffStore`, partial staging, encoding overrides, the
  fullscreen `Modal` and global keybindings (all app concepts, wrong for a
  plugin form node).
- **Hunk source — plugin supplies pre-diffed hunks.** FE-only, zero Rust, zero
  new libraries: the node carries `hunks: FormDiffHunk[]`, each a list of
  `{ kind, content }` lines. `form-nodes/diff.ts::normalizeDiffHunks` fills the
  per-line `old_lineno`/`new_lineno` (counting from `old_start`/`new_start`,
  default 1) and synthesises the `@@ … @@` header, so the Lua side stays terse.
- **Layout — unified + split, local toggle.** A per-node `$state` (init from the
  node's `mode`, default `unified`), *not* the app-wide `diffStore`, so two diff
  nodes are independent and toggling one never touches the git diff panel.
  `hide_mode_toggle` hides the control.
- **Display-only.** `diff` extends `FormNodeBase` (no `name`) — it is not
  collected into `values`. It updates live via the §3.2 `patch` op (`merge` new
  `hunks` onto the node by its stable `id`), exercising that channel.
- **Highlight.** Reuses the Prism setup via `diff-formatter.highlight(content,
  path)`. A new additive export `syntheticPathForLang(lang)` (reverse of the
  existing `EXT_TO_LANG`) lets the node drive the grammar from a `language` id
  when it has no `path`.

### Landed (step 5 — `data_tree`)

The third host widget — shipped as a **dynamic mode of the existing `tree` node**
(not a new type), so static trees are untouched and the dynamic opt-ins are
purely additive. **Design decisions (closed):**

- **Extend `tree`, don't fork.** `FormFieldTree` gains `lazy` / `on_expand` /
  `on_select` / `virtualize_threshold` / `row_height` / `on_scroll_range`;
  `FormTreeNode` gains `id` / `has_children` / `loading`. Absent ⇒ today's
  behaviour. The renderer moved out of the `FormNodeField` catch-all into a
  dedicated `form-nodes/FormNodeTree.svelte` (routed before the catch-all,
  beside `editor`/`diff`), reusing the shared `.pf-tree-*` styles.
- **Lazy children = scoped `on_expand` + `patch` (no new host API).** Expanding
  a row that has `has_children` but no loaded `children` fires the scoped
  `on_expand` slot (`{ id, value, path }`) and shows a spinner row; the plugin
  responds with `arbor.ui.form.patch` that `merge`s `children` onto the row and
  clears `loading`. This required one addition to the patch applier: a `tree`
  node's `nodes` array is now walked by `childArraysOf`, so a `FormTreeNode` is
  addressable by its own stable `id` (children themselves are caught by the
  generic `children` descent). Prefer `merge`/`set` over `append` for tree rows
  (`append` runs `normalizeNode`, which is for FormNodes, not FormTreeNodes).
- **Value-bearing + scoped `on_select`.** Selection stays in `values[name]`
  (string, or `string[]` in `multi`) and submits like any field; `on_select`
  ships the new value on the scoped channel (preferred over the legacy
  whole-form `change_action` when both are set). `scope_state` rides along.
  Inline edit is **deferred** (reserved, not implemented this step).
- **Flatten-then-window rendering.** Rows are rendered from a flat list of the
  currently-visible (expanded) rows so virtualization and roving-focus keyboard
  nav share one model. Above `virtualize_threshold` (default 400) the list is
  windowed with fixed `row_height` (default 24) like `VirtualHunk`; an optional
  `on_scroll_range` slot ships `{ start, end, total }` for window-driven fetch.
- **Keyboard-first.** The tree is a focusable `role="tree"`; ↑/↓ move, →/←
  expand-or-descend / collapse-or-ascend, Home/End jump, Enter/Space select,
  with `aria-activedescendant` + scroll-into-view.

**Implementation (FE-only — no Rust/Tauri change; reuses §1–4):** new
`form-nodes/FormNodeTree.svelte`; routing in `FormNodeRenderer`; the `tree`
branch + recursive snippet removed from `FormNodeField`; `FormTreeNode` /
`FormFieldTree` extended in `types/plugin.ts`; one branch added to
`patch.ts::childArraysOf`; `FormBuilder.tree` added in `builders.lua`.

The remaining widget (full-view container) is still sketch-only.

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
3. ✅ **Patch ops** (`patch`, `set_state_path`) — additive update ops.
   *(landed — see §3.2 "Landed (step 3)")*
4. ✅ **Scoped per-node events + per-node concurrency.**
   *(landed — see §3.1 "Landed (step 4)")*
5. **Host widgets** (editor/diff/tree/full-view) consuming §1–4.
   - ✅ **`editor` (landed)** — CodeMirror 6, value-bearing + scoped
     `on_edit`/`on_select`. See §4 "Landed (step 5 — `editor`)".
   - ✅ **`diff` (landed)** — read-only diff viewer reusing the app's diff row
     renderers; plugin-supplied hunks, unified + split, display-only, updated
     live via `patch`. See §4 "Landed (step 5 — `diff`)".
   - ✅ **`data_tree` (landed)** — shipped as the `tree` node's dynamic mode:
     lazy children via scoped `on_expand` + `patch`, scoped `on_select`,
     keyboard nav, virtualization. See §4 "Landed (step 5 — `data_tree`)".
   - ⏳ full-view container — sketch only.

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
- ~~First host widget + its library / value model / slots.~~ **Decided
  (step 5): `editor` first**, built on the existing CodeMirror 6
  (`StudioTextPane`) — no new dependency — value-bearing + scoped
  `on_edit` (debounced) / `on_select`. See §4 "Landed (step 5 — `editor`)".
- ~~`diff` widget: hunk source / renderer / layout.~~ **Decided (step 5):
  plugin supplies pre-diffed hunks** (FE-only, no Rust/lib), rendered by a lean
  `FormNodeDiff` that **reuses the existing `DiffHunk`/`VirtualHunk`** (not the
  app's `DiffViewer`), with a **local unified/split toggle**. Display-only;
  updated live via the `patch` op. See §4 "Landed (step 5 — `diff`)".
- ~~Patch addressing: JSON-pointer over the node tree by id, vs a flat
  `id → partial-node` map.~~ **Decided (step 3): id-keyed ops** with verbs
  (`merge`/`set`/`append`/`remove`) — more expressive than a flat merge map
  (supports append/remove of children) without JSON-pointer's brittle global
  indices. `set_state_path` uses an array of segments. See §3.2.

## 8. Docs / SDK to update when implementing

- `sdk.d.lua` (arbor-extensions repo): `arbor.command.fire`, extended
  `arbor.command.register`, dispatch union on node slots, new node event
  slots, `command_invoke` permission.
- In-app docs (`PluginDevApiUI.svelte`, `PluginDevHooks.svelte`,
  permission docs): command invocation, dispatch, event/patch.
- `hook_catalog.rs`: unaffected (no new hooks) — note in PR.
- `CHANGELOG.md` `[Unreleased]`: user-facing additions per step.
