# MCP integration — design analysis (bennu + tyto)

Status: **phases 1–3 landed** (see §8) — `arbor-http`, `arbor-mcp`, `#[handler(mcp(...))]`,
the `__tools` seam, the first bennu/tyto tools, the launcher endpoint, and the whole
permission model with its settings and consent UI. Phase 4's addressing facade
(`bennu_symbol_at`, `bennu_find_symbol`) has landed too, and of phase 5 the MCP
Status: phases 1–5 have landed apart from three additive pieces — the **SSE arm** (server→client notifications), **standalone mode** (a bridge that spawns its own backend with the launcher closed), and **plugin-declared tools** (`arbor.mcp.tool{…}` from Lua, which also needs `sdk.d.lua` in the extensions repo and `PluginDevelopment.svelte` in the same turn). Everything else is in place and verified end-to-end against a real client: the endpoint, the four gates, per-project rules, consent, the call log, resources, and the in-app docs. The invariants that must not regress are restated in `CLAUDE.md` § "Superficie AI (MCP)".
plugin-declared tools. The design below is what was built; where a
decision changed during implementation the section says so. Scope: how Arbor exposes its backends
as MCP tools to an external AI client (Claude Code / Claude Desktop), what the
`#[arbor_rpc::handler]` macro must grow to make that safe, and which bennu/tyto verbs
are worth exposing (plus the ones that are missing today).

Read alongside `docs/backend-architecture.md` (the RPC seam) and `docs/ipc-design.md`.

---

## 1. What MCP actually demands

MCP is JSON-RPC 2.0 over one of two transports (**stdio**: the client spawns a child and
frames messages on its stdin/stdout, newline-delimited; **streamable HTTP**: the client
POSTs to a URL, server may reply SSE). A server advertises capabilities in `initialize`,
then answers `tools/list` and `tools/call` (plus optionally `resources/*`, `prompts/*`).

A **tool** is roughly:

```jsonc
{
  "name": "bennu_find_usages",              // stable identifier, [a-z0-9_]
  "title": "Find usages",                    // human label (optional)
  "description": "…what it does, when to use it, what it does NOT return…",
  "inputSchema": { "type": "object", "properties": { … }, "required": [ … ] },
  "outputSchema": { … },                     // optional; enables structuredContent
  "annotations": {                           // hints, NOT security guarantees
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  }
}
```

`tools/call` returns a list of **content blocks**: `text`, `image` (base64 `data` +
`mimeType`), `audio`, `resource_link`, embedded `resource` — plus an `isError` flag and
optional `structuredContent`.

Three consequences fall straight out of this and shape everything below:

1. **A JSON Schema is mandatory per tool.** Today the seam carries only a method *name*
   (`Frame::Hello { methods: Vec<String> }`, `crates/foundation/ipc/src/transport.rs`).
   There is no arity, no types, no docs. This is the single biggest gap.
2. **The description is the prompt.** Tool selection quality is dominated by the
   description text. A handler's `///` doc comment is the natural, already-written source.
3. **Images are first-class.** Tyto's `take_screenshot` returning a *file path* is nearly
   useless to a remote model; returning an `image` content block is transformative.

---

## 2. Where the MCP server lives

The idea in the brief — *launcher is the MCP connector, the spawned program provides the
methods* — is the right shape. The open question is the wire between the MCP client and the
launcher.

**MCP is not stdio-only.** The spec defines two transports: **stdio** (the client spawns a
child process and frames JSON-RPC on its stdin/stdout) and **Streamable HTTP** (the client
POSTs JSON-RPC to a URL; the server answers either `application/json` or an SSE stream).
The older HTTP+SSE transport is deprecated but still accepted by many clients. Claude Code
supports both shapes (`claude mcp add --transport http <name> <url>`).

That matters, because a GUI app cannot itself *be* a stdio server — Claude Code spawns the
stdio process, and Arbor's launcher is already running (and reduces to the tray). Under
stdio the launcher must therefore be reached through a bridge process; over HTTP it can be
reached directly.

| # | Shape | Pros | Cons |
|---|---|---|---|
| A | Thin `arbor-mcp` binary that **spawns its own `bennu-be`/`tyto-be`** children via `ChildClient` | Zero new transport, zero new deps, works with Arbor closed | Second `bennu-be` process → duplicated symbol index (RAM, warm-up); no shared session with what the user sees; tyto capture would fight the GUI over devices |
| B | Launcher serves **Streamable HTTP** on `127.0.0.1:<port>/mcp` | Native MCP transport, **no bridge process at all**, multiple clients, shares the live BEs | The shell must speak enough HTTP: POST + body, `Accept` negotiation, `Mcp-Session-Id`, `Origin` validation |
| C | Thin `arbor-mcp` **bridge**: stdio↔MCP outward, framed-JSON over a loopback socket inward to the running launcher | Reuses `transport.rs` framing verbatim; shares the live BEs; the socket is trivial | An extra process and an extra hop to build and version |
| D | Reuse `tauri-plugin-single-instance` argv forwarding | No new anything | One-way only (no response channel) — **not viable** for RPC |

**Recommendation: B.** It is the only option with no intermediary at all, and the "needs an
HTTP dependency" objection that would normally sink it does not apply here — the repo
already hand-rolls both halves:

- **Server-side HTTP**: `crates/platform/auth/src/oauth2/installed_app.rs` binds a tokio
  `TcpListener` on `127.0.0.1`, reads the request into a buffer, parses what it needs, and
  writes the response by hand. Precedent, and a working one.
- **SSE framing**: `crates/wasm/brp/src/sse.rs` parses `text/event-stream` by hand, with an
  explicit note that pulling `eventsource-stream` wasn't worth it. That's the client side;
  the server side (`data: {json}\n\n`) is smaller.

Be honest about the delta, though: the OAuth listener handles exactly one GET and closes.
Streamable HTTP wants POST with a body (`Content-Length` / chunked), `Accept:
application/json, text/event-stream` negotiation, an `Mcp-Session-Id` header, and — an
explicit spec requirement for local servers — `Origin` validation against DNS-rebinding.
Call it 300–400 hand-written lines instead of 50, or an `axum` dependency (**ask first**,
hard rule 7).

**The phase-1 simplification that makes B cheap:** if the server never needs to push
server→client messages — no `notifications/tools/list_changed`, no progress, no sampling —
it may answer every POST with `application/json` and never open an SSE stream. That is
spec-conformant, and it collapses the implementation to a POST handler plus a body reader.
Notifications become interesting later (index warm, recording finished, project closed);
add the SSE arm then, on the `sse.rs` framing that already exists.

Keep **A as an explicit fallback mode**, not as the primary: an agent may want to ask bennu
about a project while Arbor is closed. Same `Dispatcher`, same handlers, different owner of
the child process. C stays on the table only if a client turns up that speaks stdio and
nothing else — the bridge is then ~100 lines over the same HTTP endpoint.

```
Claude Code ──POST /mcp (JSON-RPC over HTTP)──▶ arbor (launcher/shell)
                     127.0.0.1 + Origin check + token   │  consent, audit, config
                                                        ▼
                                            Router / SplitBroker  (unchanged)
                                                        │
                                                        ▼
                                     bennu-be / tyto-be  ── #[handler] fns (unchanged)
```

Everything below the launcher is **unchanged**: the endpoint terminates MCP, translates
`tools/call` into the existing `rpc(program, method, params)` path, and the router does what
it already does. The launcher stays the policy point — which is exactly where consent and
audit belong.

> Port + token live in `arbor/profiles/<p>/mcp.json` (`0600`), written on enable, so the
> `claude mcp add` line is stable and the user can copy it out of the Settings panel.

---

## 3. The macro change

### 3.1 What exists

```rust
pub struct Entry {
    pub program: &'static str,
    pub name:    &'static str,
    pub kind:    Kind,          // Sync(CallFn) | Async(AsyncCallFn)
}
inventory::collect!(Entry);
```

`#[handler]` accepts `#[handler]`, `#[handler("x.y")]`, `#[handler(program=…, name=…)]`.
The generated thunk decodes each named arg with `decode_field` and serializes the result.

### 3.2 What it must grow

Add an **optional, opt-in** `mcp` metadata block. Opt-in is non-negotiable: 164 bennu
handlers auto-exposed would be both a security hole and a context-window disaster.

```rust
#[arbor_rpc::handler(mcp(
    title  = "Find usages",
    safety = read,                    // read | write | destructive
    // description defaults to the fn's own /// doc comment
))]
/// Find every resolved use site of the symbol at `file`:`offset`.
/// Returns at most `limit` locations (default 200), each with file, line and a snippet.
fn bennu_references(state: &BennuState, args: ReferencesArgs) -> Result<Option<UsagesResult>, String> { … }
```

and the entry grows a nullable descriptor:

```rust
pub struct ToolMeta {
    pub title:       &'static str,
    pub description: &'static str,          // from /// unless overridden
    pub safety:      Safety,                // → readOnlyHint / destructiveHint
    pub idempotent:  bool,
    pub schema:      fn() -> serde_json::Value,  // input JSON Schema, built lazily
}
pub struct Entry { pub program: …, pub name: …, pub kind: …, pub mcp: Option<ToolMeta> }
```

Three design points worth deciding explicitly:

**(a) Where the description comes from.** The macro can read `#[doc]` attributes off the
`ItemFn` — they are ordinary attributes in `func.attrs`. Using the doc comment means the
rustdoc and the model prompt cannot drift, which is worth a lot. Allow
`description = "…"` to override when the rustdoc is written for a Rust reader rather than
an agent (frequent enough to need the escape hatch).

**(b) Where the schema comes from.** Three candidates:

| Approach | Cost | Verdict |
|---|---|---|
| Hand-written JSON in the attribute (`schema = r#"{…}"#`) | Zero deps, total control | Drifts from the struct the day someone adds a field. Fine as an escape hatch only |
| Macro infers from the Rust type tokens (`String` → `"string"`, `Option<T>` → not required, `Vec<T>` → array, anything else → `{}`) | Zero deps, ~80 lines in the macro | Good enough for **flat-arg** handlers; blind to struct-arg handlers, which is most of bennu |
| `schemars::JsonSchema` derived on the args struct | Correct, recursive, keeps field docs | **New direct dependency** — `schemars` is already in `Cargo.lock` transitively but adding it to a crate needs your go-ahead (hard rule 7) |

Recommendation: **schemars on the args structs**, with the inferred-from-tokens path for
flat-arg handlers so simple verbs need no derive, and the literal-JSON escape hatch for the
rare shape neither covers. `schemars` also picks up `///` on struct fields, so
`ReferencesArgs { /// Absolute path… pub file: String }` documents itself into the schema —
that per-parameter documentation is what makes tools usable, and hand-writing it for 40
tools is how it rots.

**(c) The `args` wrapper is a wart the MCP layer must hide.** The seam keys params by the
handler's *parameter name*. Bennu handlers universally take a single `args: XxxArgs`, so
the wire shape is `{"args": {…}}` — the FE knows this (`src/lib/ipc/bennu/index.ts` says so
in a ⚠️ note). Tyto is **mixed**: `select_region(args: SelectRegionArgs)` and
`start_recording(args: StartRecordingArgs)` vs `rename_capture(id: String, name: String)`
and `enumerate_ui_elements(monitor_id: String)`.

An MCP tool must not expose `{"args": {...}}` — a model given a schema with a single
`args` object property will produce more malformed calls, and the nesting is meaningless to
it. So: when a handler's only non-context parameter is a struct, the macro emits **that
struct's schema flat** as the tool's `inputSchema`, and the dispatch layer re-wraps
`{…}` → `{"args": {…}}` before calling. One flag on `ToolMeta` (`wrap_in: Option<&'static
str>`) carries it. Cost: ~10 lines. Benefit: the MCP surface is uniform even though the
internal convention is not.

> **Flagged (divergent pattern):** the flat-args vs `args:`-struct split isn't only an MCP
> problem — it means every FE call site must know which convention a given handler uses,
> and `bennu/index.ts` has a warning comment doing that job. Worth converging on one
> (struct-args reads better for anything past 2 params) independently of this work.

### 3.3 Discovery across the seam

`Frame::Hello { methods: Vec<String> }` cannot carry descriptors, and widening it is a
protocol break with an ordering landmine attached (landmine #4 in `backend-architecture.md`
— nothing may precede `Hello`). Don't touch it.

Instead add **one reserved method per BE**, `__tools`, served by a handler that walks
`inventory::iter::<Entry>()` and returns the descriptors:

```
bridge → shell → BE:  rpc("bennu", "__tools", {})
        ← [{name, title, description, inputSchema, annotations}, …]
```

Zero protocol change, cached by the bridge for the process lifetime, and it composes with
the existing `Dispatcher` (`arbor-be` can register it generically for every product, next
to `be_ping`). A BE that predates the change simply answers `unknown method` and exposes no
tools — clean degradation.

---

## 4. Three problems that will bite regardless of transport

**(a) Session state.** Most bennu verbs assume a project was opened
(`bennu_open_project` seeds `IndexService::global()`), and completion/diagnostics/rename
answer from that index. An MCP call arrives cold. Options: the MCP tool takes `root` and
the layer auto-opens (idempotent, matches how the FE behaves), or an explicit
`bennu_mcp_attach(root)` tool that must be called first. Auto-open is friendlier; it needs
`bennu_open_project` to stay cheap when the project is already open, and it needs the
"index still building" state to be a *legible* answer ("index warming, N/M files, retry in
a moment") rather than an empty result the model reads as "no usages".

Same for tyto: `start_recording` returns a session id and the engine is a process-global
singleton — an agent starting a recording while the user is recording must fail loudly, not
silently clobber.

**(b) Caret addressing.** Bennu's navigation verbs are addressed by `(file, offset)` —
a byte offset. A model does not have byte offsets; it has names, and at best `file:line`.
Exposing `bennu_references(file, offset)` verbatim guarantees garbage calls. This is the
strongest argument for the MCP surface being a **curated facade**, not a 1:1 dump: a
handful of new handlers that take `(file, line, column)` or `(symbol_name)` and resolve to
an offset internally. See §5.

**(c) Output budget.** `bennu_project_tree`, `bennu_index_entries`,
`bennu_library_classes`, `bennu_find_in_files` can return megabytes. Every MCP result lands
in the model's context. The macro/dispatch layer should enforce a uniform envelope: a
`max_items`/`cursor` pair on list-shaped tools, plus a hard byte cap at the bridge that
truncates and appends a `"…truncated, N of M shown, refine your query"` note. Better to bake
this into the MCP layer once than to add a `limit` param to 40 handlers ad hoc.

---

## 5. Bennu — what to expose

164 handlers exist. Exposing ~25 is the right order of magnitude. Tiering:

### Tier 1 — expose first (read-only, high value, cheap)

| Tool | Backing handler | Note |
|---|---|---|
| `bennu_open_project` | as-is | idempotent; returns capabilities + JDK + module model |
| `bennu_project_tree` | as-is | needs a `depth`/`max_nodes` cap |
| `bennu_read_file` | as-is | already encoding-aware — a real advantage over a raw file read (Cp1252 legacy sources) |
| `bennu_find_in_files` | as-is | needs `max_results` |
| `bennu_class_index` | as-is | "go to class" by name — the model's natural entry point |
| `bennu_diagnostics` / `bennu_project_diagnostics` | as-is | the highest-value verb: real semantic errors without a compile |
| `bennu_type_shape` | as-is | "what's in this DTO" — one level, bounded |
| `bennu_dependencies` | as-is | reads poms only, never runs Maven |
| `bennu_index_stats` | as-is | lets the model know whether the index is warm |
| `bennu_todos` | as-is | cheap, bounded |
| `bennu_ssr_search` | as-is | structural search — a genuinely unique capability no grep gives an agent |

### Tier 2 — expose after the addressing facade exists

`bennu_references`, `bennu_declaration`, `bennu_hover`, `bennu_inherited_members`,
`bennu_symbol_tree_of`, `bennu_intentions_at`, `bennu_form_analysis`,
`bennu_action_property_target`, `bennu_mybatis_nav`. All caret-addressed → each needs the
name/line-column front door (§4b).

### Tier 3 — expose behind explicit consent, `destructiveHint: true`

`bennu_write_file` (has a stamp guard — good), `bennu_rename_apply`, `bennu_ssr_apply`,
`bennu_new_file`, `bennu_move_to_package`, `bennu_build`, `bennu_run`, `bennu_run_tests`,
`bennu_hotswap_jsp`, `bennu_reindex`. `bennu_run`/`bennu_build` are arbitrary code
execution by construction — they deserve a per-call confirmation in the launcher, not a
config toggle.

### Never expose

`bennu_debug_*` (a live debug session driven by a remote model is a hazard with no payoff),
`bennu_download_dictionaries`/`bennu_download_sources` (network + disk),
`get/set_bennu_config`, `set_*_config`, `bennu_did_change` (buffer-sync protocol, meaningless
out of the editor), the whole `bennu_lsp_*` family (that's an LSP client — the agent's own
tooling already speaks LSP if it wants to).

### Missing — worth adding for an agent

1. **`bennu_symbol_at(file, line, column)` → resolved symbol + offset.** The bridge between
   how a model addresses code and how bennu does. One handler unlocks all of tier 2.
2. **`bennu_find_symbol(query, kind?)` → fqcn/member candidates.** `bennu_class_index` covers
   types only; a member-level "where is `calcolaImponibile` defined" has no verb today.
3. **`bennu_apply_edits(file, edits[])` with the stamp guard.** `bennu_write_file` is
   whole-file: a model rewriting a 2000-line class to change one method is both expensive
   and destructive. A range-edit verb (bennu already models `SourceEdit`) is safer and
   cheaper.
4. **`bennu_explain_diagnostic(file, diag_id)`** — diagnostics + the applicable intentions in
   one round trip, so the model doesn't need three calls to act on an error.
5. **`bennu_project_summary(root)`** — one call returning module tree + frameworks detected +
   JDK + build status + counts. The natural first tool call of any session; today it's four
   calls (`open_project`, `capabilities`, `index_stats`, `dependencies`).
6. **A "index ready" wait/poll verb.** `bennu_index_stats` reports a snapshot; there is no
   "block until warm or timeout", so an agent must busy-poll.

---

## 6. Tyto — what to expose

Tyto is smaller (24 handlers) and much more interesting per-verb, because it gives a model
*eyes on the desktop*.

### Expose

| Tool | Backing handler | Note |
|---|---|---|
| `tyto_list_sources` | `list_capture_sources` | monitors + windows with titles/apps |
| `tyto_screenshot` | `take_screenshot` | **must return an image content block, not a path** |
| `tyto_read_ui_tree` | `enumerate_ui_elements` | accessibility rects — lets a model reason about the screen structurally instead of only pixel-wise. Windows-only today |
| `tyto_window_rects` | `enumerate_window_rects` | |
| `tyto_session_state` | `session_state` | |
| `tyto_list_captures` | `list_captures` | |

### Expose with consent / `destructiveHint`

`start_recording`, `stop_recording`, `pause_recording` (recording the user's screen on a
model's initiative is exactly the class of action that needs a visible, per-session
approval — plus a persistent on-screen indicator), `remove_capture`, `clear_captures`,
`rename_capture`.

### Never expose

`get/set_tyto_config`, `reveal_capture` / `reveal_output` / `open_capture` (they drive the
user's shell through the reverse channel), `freeze_screen`/`select_region`/`clear_region`
(overlay-internal state machine, meaningless outside the picker).

### Missing — worth adding

1. **Image bytes, not paths.** `take_screenshot` returns `Result<String>` = a filesystem
   path. For MCP it must come back as `{type: "image", data: <base64>, mimeType: "image/png"}`.
   Two ways: a new `take_screenshot_bytes` handler, or a bridge-side "read the returned path
   and inline it" rule. The former is cleaner and also useful to the FE; the latter is free.
   Either way it needs a **downscale + budget cap** (a 4K PNG is millions of tokens) —
   propose `max_width`/`format=webp|png|jpeg` params with a sane default (e.g. long edge
   1568px, matching what vision models actually consume).
2. **`tyto_screenshot_window(title_or_id)`** — today capture is `target_kind` + `source_id`
   (`win-<hwnd>`). A model has a window *title*. Resolving title → id belongs backend-side.
3. **A capture-with-annotations verb**: rects/labels drawn onto the screenshot from the UIA
   tree, so the model can refer to element #7 rather than to coordinates. Cheap to do (the
   rects already exist), disproportionately useful.
4. **`tyto_record_for(seconds)`** — a bounded, self-terminating recording. An agent that must
   pair `start`/`stop` around a long-running action will eventually leak a recording; a
   bounded verb can't.
5. **OCR** is the obvious gap for non-accessible apps (Java/Swing legacy, exactly bennu's
   target audience). Would need a new dependency — **do not add without asking**; and note
   that a vision-capable model reading the raw screenshot covers most of the need already.

**Explicitly not proposed:** input injection (click/type). Tyto is a recorder; making it a
remote-control surface is a different product decision with a very different threat model.
If you ever want it, it should be its own opt-in binary, not a tyto verb.

---

## 7. Consent, audit, config

The launcher being the connector is what makes this governable. Minimum viable policy:

- **Per-product master switch**, off by default: `[mcp] enabled`, `[mcp.bennu] enabled`,
  `[mcp.tyto] enabled` — a typed section in `AppConfig` (profile.toml), per rule 11, with
  `get_mcp_config`/`set_mcp_config` + a store + a Settings panel. Never localStorage.
- **Safety tiers gate on config, not on trust in the model**: `read` tools run silently;
  `write` tools require the product switch plus an "allow writes" toggle; `destructive`
  tools raise a real Arbor modal (`ConfirmModal`) with the tool name and its arguments, and
  answer with `isError: true` on denial or timeout. A "remember for this session" checkbox
  keeps it usable.
- **Path scoping.** Bennu tools that take a `root`/`file` must be confined to the open
  projects (or a configured allowlist) — the same idea as the plugin manifest's `fs_scope`.
  Without it, `bennu_read_file` is an arbitrary-file-read primitive for any process that can
  reach the socket.
- **Audit log** in the launcher: timestamp, tool, arguments, allowed/denied, duration. This
  is also the debugging surface when the model does something surprising, and it should be
  visible in the UI (a panel next to Jobs), not only on stderr.
- **Token + loopback binding** on the socket, and the token file `0600` under the profile
  dir. Any local process can otherwise drive Arbor.

---

## 8. Suggested phases

1. ~~**Seam**: `ToolMeta` on `Entry` + `mcp(...)` on the macro + `__tools` handler in
   `arbor-be`. Annotate ~6 bennu read-only handlers as a proof.~~ **Done.** Two things
   were added during the build that the analysis had not foreseen: `mcp(name = …)`,
   because tyto's handlers carry no product prefix and `list_captures` would collide
   across products, and `mcp(output = json|text|image)`, because a screenshot has to come
   back as an image block and that decision belongs next to the handler rather than in a
   hardcoded list in the host.
2. **Endpoint**: Streamable HTTP on `127.0.0.1:<port>/mcp` in the shell — JSON-only
   responses (no SSE arm yet), `Origin` check, token, port file. Wire `initialize` +
   `tools/list` → `__tools`, `tools/call` → `Router::call`. Read-only tools only, hard byte
   cap on results.
3. **Policy**: `[mcp]` config section + Settings panel + consent modal + audit log. Unlock
   the `write` tier.
4. **Ergonomics**: the bennu addressing facade (`bennu_symbol_at`, `bennu_find_symbol`,
   `bennu_project_summary`), tyto image content blocks + window-by-title.
5. **Later**: the SSE arm (server→client notifications: index warm, recording finished);
   standalone mode (a bridge that spawns its own BE when the launcher is closed);
   MCP *resources* for open project files; plugin-declared tools
   (`arbor.mcp.tool{…}` from Lua — the plugin host already has hooks + a manifest opt-in
   model that maps onto this almost exactly).

---

## 9. Open questions for you

1. **HTTP by hand or with `axum`?** The endpoint is 300–400 hand-written lines in the style
   of `installed_app.rs`, or a dependency. I lean hand-rolled for the JSON-only phase 1 —
   it stays inside the precedent the repo already set — and revisit if the SSE arm and
   session handling make it ugly.
2. **`schemars`** as a direct dependency of `arbor-rpc` (or of a new `arbor-rpc-schema`)?
   Everything else about schema generation is a compromise.
3. **Bennu standalone mode** — is "ask bennu about a project with Arbor closed" a goal, or
   is MCP explicitly an augmentation of a running session? It changes phase ordering.
4. **Tyto consent granularity** — is a screenshot a `read` tool (silent, gated only by the
   product switch) or does every capture warrant a prompt? My instinct: silent while a
   visible indicator is on screen, prompt for recording.
5. **Naming**: `bennu_*`/`tyto_*` prefixes on tool names (matching the wire methods), or a
   product-neutral verb namespace (`code.find_usages`, `screen.capture`) that reads better
   to a model? The former is less work and traceable; the latter is more legible.
