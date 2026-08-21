# The plugin surface — wasm, frames, and why plugin UI is still built in

An analysis, not a plan. Everything numeric here was measured against the tree on the
`feature/bennu` branch; where something is an inference rather than a measurement it says so.

The four questions, in the order they actually depend on each other:

1. Should wasm plugins run alongside Lua?
2. Should a plugin be able to contribute a **frame** — the Bevy shader preview being the case
   that prompted it?
3. Why is plugin UI still almost entirely built in?
4. What is missing, and what would adding it cost?

The short version: **(1) no as a plugin language, yes as a packaging format for interfaces
Arbor itself defines — and three of those are already earmarked in the tree; (2) yes but last;
(3) not for the reason it looks like; (4) ten widgets and three content nodes, and they are
cheap.**

---

## 0. The measurements

| | |
|---|---|
| FormNode types today | **72** — 22 field, 50 layout |
| `FormNodeRenderer.svelte` | **1232 lines**, one file |
| Modals in `src/lib/components` | **148** |
| …of those, under `plugins/` | **10** |
| Lua `arbor.ui.*` entry points | ~48 across 13 files |
| Lua API namespaces | **23** |

Two of these deserve to be read together. Seventy-two node types is not a thin vocabulary —
it mirrors most of `shared/ui` and `shared/internal`, down to `encoding_pill` and
`provider_user_badge`. And yet 138 of the 148 modals in the app are built in.

**So the reason plugin UI is built in is not that plugins have no widgets.** That was the
hypothesis this analysis started from and the numbers do not support it. The real reasons are
in §3, and they are more fixable.

---

## 1. wasm alongside Lua

### Two propositions, and only one of them is bad

The question splits, and merging the halves is what makes it hard to answer.

**A. wasm as a language for plugin authors.** Somebody writing "highlight the TODOs" chooses
Rust instead of Lua. The guest then needs the host API — 23 namespaces, ~48 entry points in
`arbor.ui` alone, all of it fluent and table-shaped by design (`arbor.notify{title=…,
level="warning"}`, builder chains, config tables with a dozen optional keys). None of that
shape survives an ABI that moves bytes. Every function becomes a serialization format, a host
import, a guest wrapper that rebuilds the ergonomics, and a version story — and then **both**
APIs are maintained forever, in step. That is the largest available violation of *un solo
pattern per concetto* in this codebase.

Against it, besides: the runtime is a heavy dependency in a workspace that already builds
slowly; a wasm trap has an offset where a Lua error has a line and a stack; `arbor-extensions`
becomes two ecosystems a reader has to know both of. And **no plugin is slow because it is
Lua** — plugins react to hooks, build forms and call HTTP, and mlua is the bottleneck in none
of them.

**Verdict on A: no.**

**B. wasm as a packaging format for an interface the host defines.** A Studio format backend
does not call `arbor.ui.form`. It receives text and returns a document model. The host API it
needs is *nothing* — it implements one trait.

**Verdict on B: yes, and the tree already says so.**

### The three candidates already earmarked

They are not hypothetical and they are not equivalent.

| | why not Lua | interface shape | I/O needed |
|---|---|---|---|
| **cloud** | SigV4, GCS OAuth refresh, multipart, opendal's retry | **coarse** — `list`/`stat`/`download`/`upload`/`copy`/`delete` | yes |
| **studio** | 5 format backends, thousands of lines of Rust | **chatty** — 36 methods, several per-node | no |
| **brp** | stated in its own manifest: *"long-lived SSE streaming + tokio task lifecycle need native async that Lua can't provide"* | streaming, continuous | yes |

The traits are already shaped for a boundary without anyone aiming at one:
`StudioFormatBackend` is `async`, `Send + Sync`, takes `doc_id: &str` handles rather than
references, and every type crossing it (`ParseResult`, `NodeView`, `QueryHit`,
`StudioMutation`) is serializable.

### The cons that remain, per candidate

**cloud — the right pilot.** Coarse interface, big payloads, and the alternative is not merely
awkward but impossible. Two real costs: it needs host-provided HTTP or sockets (WASI), which
is where the "narrow interface" promise widens; and TLS has to sit on one side or the other.
In the guest means shipping rustls to wasm and needing raw sockets. On the host means **the
host holds the credentials** — which here is the better answer, not the worse one: in Model D
the shell already *is* the credential broker, so a guest asking the host to perform a signed
request never holds a token, which beats a Lua plugin with `arbor.http` and a token in scope.
Separately, opendal in wasm is a port and not a recompile: its backends want reqwest + tokio,
and the workspace pins `reqwest-rustls-tls`.

**studio — easiest, least urgent.** Pure compute: no capability to grant, the simplest guest a
runtime can host. But the trait has 36 methods and some are per-node — `get_children(doc_id,
path)`, `get_value(doc_id, path)` — so what is a function call today becomes a serialized round
trip **per tree expansion**. A wasm boundary rewards coarse interfaces and punishes chatty
ones, and this one was designed to sit in-process. As a guest interface it would want a
coarser shape: return a subtree rather than a node, batch mutations. What is gained is real
but modest — a third party could add `.ini`, `.plist`, `.env` or protobuf-text without an
Arbor release, and a parser bug in a malformed file could no longer take the app down.

**brp — best justified, hardest shape.** An SSE stream pushing events continuously is the
worst case for a guest: streaming *outward*, indefinitely. Last, despite having the clearest
written reason.

### What it costs regardless

The runtime. There is **no wasm anything** in the workspace today — no wasmtime, no wasmer, no
wit-bindgen, no `wasm32` target, no pipeline. `crates/wasm/` is a declared destination, not
infrastructure. Whoever does the first one builds all of that.

### A naming problem that caused this confusion

None of the three has a `main.lua`, subscribes to a hook, or builds a form. They implement a
trait the host defines. **They are providers, not plugins** — and they probably want a
different extension point than the Lua marketplace, rather than the same manifest stretched to
cover both. Calling both "plugin" is exactly what let proposition A and proposition B get
merged.

### Order

**cloud → studio → brp.** cloud proves the runtime on the case where Lua is not an option and
the interface is already the right shape; studio is the one to reshape before porting; brp
last, because streaming out of a guest is the part nobody should learn the runtime on.

---

## 2. A frame node on the frontend

The Bevy shader preview needs B: a wasm module in the page, drawing to a canvas. The question
is whether that arrives as a plugin.

### The shape

Arbor already has plugin-contributed main-area views (`arbor.ui.add_view`, rendered by
`PluginViewPanel.svelte`), whose **body is a form-DSL tree**. A canvas is not a FormNode.

So one new node type, whose content is an isolated frame:

```
{ type = "frame", entry = "preview/index.html", height = 320 }
```

Served from the plugin's own directory over a custom protocol, with its own origin,
communicating by `postMessage`. This is VS Code's webview API, and Arbor does not learn the
word "wasm" — it serves an entry point.

### The cons, and they are not small

**a. The trust model widens to arbitrary JS.** Today a plugin is Lua in a Rust host with a
curated API. A frame executes whatever it ships.

*But* — and this is the finding that most surprised me — the widening is smaller than it
looks, because a hole already exists. `PluginFormModal.svelte:116` takes `form.css` and
appends it to `document.head` **unscoped**:

```js
const el = document.createElement('style');
el.textContent = form.css;
document.head.appendChild(el);
```

A plugin that writes `button { display: none }` hides every button in the application, not
only its own. That is a live issue today, independent of anything in this document, and it
should be scoped whether or not a frame node is ever built.

Still: CSS can deface, JS can act. A frame is worse. The iframe origin is what contains it,
and it must be there from the first line, not added later.

**b. The data plane has to bypass the plugin.** The control plane (which entry, which mesh)
can route `Lua → IPC → renderer → postMessage`. The data plane cannot: the shader source
changes on every keystroke and uniforms change on every pixel of a slider drag. Those must go
straight from the page to the frame. So the architecture has a documented exception in it from
day one, and that exception is where future confusion will live.

**c. It will not look like Arbor.** Theme, font scale, focus ring, `Esc`, the Command Palette
— none of them cross an iframe boundary by themselves. `arbor:theme_changed` has to be
forwarded in, focus has to be handled entering and leaving, and the *keyboard-first* rule in
`CLAUDE.md` is not negotiable. That is a list, and lists like it grow.

**d. The one that worries me most: it removes the pressure to fix §4.** Seventy-two node types
plus one node that can be anything is a DSL with an escape hatch, and escape hatches win. Why
would anyone add a `spinner` node when a frame can render a spinner? The form DSL stops
growing, plugin UI fragments into a dozen private design systems, and the thing that made
plugin panels look native — that they were assembled from the same widgets the app uses — is
gone.

That is not hypothetical. It is what happened to every plugin API that shipped a webview
before it finished its widget set.

### Verdict

**Yes, eventually — after §4, not before.** The order matters more than the decision. Build
the missing widgets first, so the frame is what you reach for when the DSL genuinely cannot
express something (a GPU canvas), not the first thing you reach for because the DSL is missing
a loading indicator.

And build the shader preview as a **built-in panel behind the same iframe and the same five
messages** (`init` · `set_shader` · `set_uniform` · `set_mesh` · `set_time`). If the node ever
lands, that panel is its first consumer and the port is mechanical. That is the crate-split
rule from `CLAUDE.md` applied to a plugin boundary.

---

## 3. Why plugin UI is still built in

Not for lack of widgets. Four real reasons, in descending order of how much they explain.

**a. Every new widget requires an Arbor release.** The node vocabulary is host-defined and
lives in one 1232-line file. A plugin can only assemble what somebody already added, so the
moment a plugin wants something slightly different, the path of least resistance is to build
the whole panel in the host instead. This is the structural answer and the other three are
consequences of it.

**b. The connective widgets are the ones missing.** See §4 — the gaps are not exotic. A plugin
panel today cannot say *loading*, cannot say *nothing here*, cannot collapse a section and
cannot show a badge. Those are not decorations; they are the states every panel spends most of
its life in, and a panel that cannot express them looks unfinished no matter how good its
table is.

**c. There is no way to show content.** No `markdown`, no `image`, no `icon_button`. A plugin
that wants to display a diagram, a screenshot or a paragraph of formatted prose cannot.

**d. Every interaction is a round trip.** A value change dispatches to Lua and the answer comes
back as a patch. That is right for a form and wrong for anything continuous — dragging,
hovering, live preview. (Inference: I did not measure the latency; the shape of the channel is
enough to rule out the continuous cases.)

The `css` field is the existing escape hatch for (a) and (b), and it should not be used for
either — it is global, so a plugin styling its own panel is styling the app.

---

## 4. What is missing

### Widgets with no node (measured, all absent from `plugin.ts`)

| Missing | In `shared/ui` as | Why it matters |
|---|---|---|
| `spinner` | `Spinner` | a panel cannot say it is working |
| `empty_state` | `EmptyState` | a panel cannot say there is nothing |
| `collapsible` | `Collapsible` | long panels cannot fold |
| `badge` | `Badge` | counts and states have no compact form |
| `log_stream` | `LogStream` | any plugin running a process rebuilds it |
| `number_stepper` | `NumberStepper` | numeric input is a bare text field |
| `split_button` | `SplitButton` | primary+menu actions have no shape |
| `sidebar_section` / `sidebar_item` | both | a sidebar panel cannot look like a sidebar |
| `searchbar` | `SearchBar` | filtering is hand-rolled per plugin |
| `knob` | `Knob` | continuous values have no native control |

Every one of these already exists as a component. The work is a node type, a branch in the
renderer, an SDK entry and a docs line — **the cheapest possible ratio of value to effort in
this whole document**, and the first four are close to mandatory.

### Content nodes with no equivalent

- **`markdown`** — formatted prose, links, lists. Plugins ship documentation as `doc.html`
  already, so the renderer exists in the marketplace modal; a node would reuse it.
- **`image`** — from the plugin's own directory, over the same protocol a frame would use.
  This is the small, safe half of the frame idea, and it covers a real fraction of what people
  reach for a frame to do.
- **`icon_button`** — an icon-only action. Currently a `button` with a label you do not want.

### Structural gaps

- **`form.css` is global.** Should be scoped to the modal's subtree (`@scope`, or a generated
  attribute selector). This is a fix, not a feature.
- **No plugin-defined node types.** The vocabulary can only grow by an Arbor release. A
  composite node — "these existing nodes, under this name, with these defaults" — would let a
  plugin build its own vocabulary out of host primitives without either a release or a frame.
  (This is speculative; I have not designed it.)
- **Keyboard behaviour inside plugin forms is unverified.** `CLAUDE.md` requires every action
  to be keyboard-completable, and I did not check whether the DSL's own nodes honour it. Worth
  an audit before adding more.

---

## 5. What I would do, in order

1. **Scope `form.css`.** A live hole, unrelated to everything else here, cheap.
2. **Add the four connective widgets** — `spinner`, `empty_state`, `collapsible`, `badge`.
   This is what makes a plugin panel stop looking unfinished.
3. **Add `markdown` and `image`.** The two content nodes, and the ones that absorb most of the
   demand a frame node would otherwise have to serve.
4. **Add the remaining six widgets** as they are actually wanted, rather than pre-emptively.
5. **Build the shader preview as a built-in panel, behind an iframe, on five messages.**
6. **Then, and only then, decide about a `frame` node** — with the evidence of what people
   still could not build.
7. **wasm providers, on their own track** — `cloud` first, because it is the one where Lua is
   not an option and the interface is already coarse enough to survive a boundary. This does
   not compete with 1–6: it is a different extension point, serving Arbor's own subsystems
   rather than plugin authors, and the only thing the two share is a marketplace entry.

The through-line is that steps 1–4 are small, unglamorous and fix the thing that was actually
wrong, and step 6 is the large architectural decision that gets easier the longer it waits.
