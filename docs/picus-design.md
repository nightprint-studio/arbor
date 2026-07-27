# Picus — design & state of play

Picus is Arbor's product for databases and the SQL scripts that build them. Two halves in
one window:

- a **database client** — several simultaneous sessions, schema browsing, query editor,
  data grid;
- a **maintainer of the script repository** those databases are installed from, where the
  same logical change lives twice: once in the Oracle branch, once in the PostgreSQL one,
  in two different syntaxes.

The bridge between the halves is the point of the product: the schema read from a live
database feeds the generation forms, and what is generated lands in the scripts on disk.

> The original brief (written by someone outside the Arbor codebase, so parts of it were
> adapted — see §3) lives in the conversation that opened the product. This document is the
> canonical state.

---

## 1. The structural invariant

**The dialect is a property of the FOLDER, never a global "current dialect".** Every branch
of the repository declares its own; every function that generates, analyses or rewrites SQL
takes it as an explicit parameter. There is no ambient dialect anywhere in the design, and
adding one would break the product's single reason to exist.

Two corollaries the code already honours:

- one generation produces N files, each correct on its own terms;
- a rule that makes sense for one role (a version guard on an update script) must never
  propagate to another (an initialisation script).

---

## 2. State: what exists today

### Frontend — complete; the database half on real RPC, the script half still on fixtures

~50 files under `src/lib/{components,stores,ipc,types}/picus/`. Precedent followed: **Tyto**
(mocked UI + window wiring landed first, backend after).

Which is which, precisely — this is the thing to know before touching anything:

| Area | State |
|---|---|
| Connections, schema tree, table tab, query editor | **Real.** `stores/picus/{connections,schema,query}.svelte.ts` → `ipc/picus/db.ts` → `picus-be`. |
| Connection modal | **Real and data-driven** — fields, labels, defaults, capabilities from `picus_providers`; an engine with no driver says so instead of disappearing. |
| Settings | **Real** (product half). Project-level settings (encoding, EOL, version table) are still in memory by design — they belong to the project's config. |
| Generator (form / paste / CSV / preview / diff) | **Still on `ipc/picus/mock-emit.ts`.** The Rust emitter and its handlers exist and are tested; nothing calls them yet. ← *first thing to do next* |
| Script tree, inventory, consistency findings, file text | **Still on `ipc/picus/mock.ts`** until the parse/inventory/analyze crates land. |

**The frontend has not been type-checked.** `svelte-check` is the user's step (project rule), so
every frontend change in this session is un-compiled. If the first run after a compact shows
Svelte/TS errors in Picus, that is where they come from — not from something mysterious.

| Area | Files |
|---|---|
| Chrome | `PicusWindow`, `PicusShell`, `shell/{PicusTitleBar,PicusStatusBar,PicusTabBar,PicusToolbar}` |
| Sidebar | `panels/{ConnectionsPanel,ScriptsPanel,GeneratePanel,InventoryPanel}` |
| Views | `views/{GenerateView,QueryView,TableView,FileView,InventoryView}` |
| Generator | `generate/{DmlValueGrid,PasteSqlPanel,CsvImportGrid,TargetEditor,SqlPreview,PatchDiffCard,AddDestinationModal}` |
| Bottom dock | `panels/{PicusBottomDock,FindingList}` |
| Modals | `PicusSettingsModal`, `PicusConnectionModal`, `PicusShortcutsModal`, `PicusAboutModal`, `PicusDocsPanel` (+ `docs/*`) |
| Small shared bits | `PicusConnectionPill`, `PicusDialectChip`, `PicusRoleChip`, `picus-shortcuts.ts`, `picus-sql-language.ts` |
| Stores | `stores/picus/{ui,connections,schema,project,tabs,dml,query,consistency,settings}.svelte.ts` |
| Types | `types/picus/index.ts` |

### Widgets contributed to the shared library

| Widget | Why it is shared |
|---|---|
| `shared/ui/DataGrid.svelte` | Virtualised, sortable, filterable, resizable. NULL ≠ empty string; numbers right-aligned with tabular figures |
| `shared/ui/Pagination.svelte` | Page controls; summary before buttons |
| `shared/ui/ColorPalettePicker.svelte` | Extracted from the pattern duplicated inline in `CreateWorkspaceModal` / `GroupFormModal` |
| `shared/ui/ShortcutsReference.svelte` | Extracted so Picus didn't fork `TytoShortcutsModal` |
| `shared/ui/ActivityBar.svelte` | Extended with a `dot` prop (marker for open findings) |
| `shared/internal/EncodingPill.svelte` | Extended with `expected` (drift → error state) and `eol` |

### Window wiring — done

`src-tauri/src/window/picus.rs`, `capabilities/picus.json`, `handlers.rs`,
`window/mod.rs` (`product_id_for_label`), `routes/+page.svelte`, `ipc/app.ts`
(`PRODUCT_WINDOW_OPENERS`), `launcher/canopy.ts`, `stores/surfaces.svelte.ts`,
`utils/window-menu.ts`, `shared/workspace/SurfaceHost.svelte`,
`shared/internal/WorkspaceTabs.svelte`.

### Temporary code — delete when the backend lands

- `src/lib/ipc/picus/mock.ts` — fixtures standing in for every read.
- `src/lib/ipc/picus/mock-emit.ts` — a TypeScript stand-in for the `picus-emit` crate.
  Deterministic and dialect-aware, but in the wrong language and the wrong process. The real
  emitter owns the golden tests; nothing here should grow a second home.

### Backend — the database half is live

`picus-be` runs and talks to PostgreSQL. Serving today: the typed product config, the
per-engine descriptors, connections, schema, paged rows, statement execution and
server-side cancellation. The script half lands in the following waves against the same
`PicusState`.

| Crate | What it holds |
|---|---|
| `crates/products/picus/types` | `picus-types`: `EngineKind` + the schema shapes. A **leaf** — serde only. Both halves depend on it and on each other not at all, which is what keeps the script half free of any dependency on drivers. |
| `crates/products/picus/ast` | `picus-ast`: `DmlModel` (no dialect field, deliberately) + `Target` (where the dialect lives). |
| `crates/products/picus/emit` | `picus-emit`: one model in, one correct statement per destination out. Owns the golden tests. |
| `crates/products/picus/core` | `PicusState` (event egress + reverse channel + the engine registry + the live-session pool), `PicusConfig`, the connection store (`connections.toml`). Tauri-free. Public API through `prelude`. |
| `crates/products/picus/db-provider/api` | `picus-db-api`: the `DbProvider` / `DbSession` traits, the wire types, `DbProviderDescriptor`, `SecretResolver`, the registry. No driver, no SQL. |
| `crates/products/picus/db-provider/postgres` | `picus-db-postgres`: the implementation over `tokio-postgres` + rustls. |
| `crates/products/picus/be` | `[[bin]] picus-be` — slim path (no plugin host). `selftest`, `config_cmds`, `connections`, `schema`, `query`, `providers`, `secrets`. |

Wiring, all in place:

- `arbor-core` — `PRODUCT_PICUS` + `picus_config_dir` / `picus_config_path` / `picus_data_dir`,
  exported from the prelude;
- workspace `members` + `package.json`'s `backends:dev` / `backends:release` +
  `scripts/stage-backends.mjs`;
- `src-tauri/src/ipc/mod.rs` — `router.register("picus", SplitBroker::pure_oop("picus"))`,
  `ensure_picus_be` + `spawn_picus_be`, emitting `arbor://picus-be-up` / `-down`;
- `src-tauri/src/window/picus.rs::open_picus_window` — `ensure_picus_be` on `spawn_blocking`
  (the blocking-pool rule is mandatory, see `docs/backend-architecture.md`);
- `src-tauri/src/window/workspace.rs::ensure_backend_for` — the `picus` arm for tabbed mode;
- frontend — `rpc.ts`'s `picus(...)` helper, `ipc/picus/config.ts`, and
  `picusSettingsStore.loadConfig()` from `PicusWindow` on mount **and** on
  `arbor://picus-be-up` (the spawn races window creation; without the second read the store
  would keep defaults and then write them back over the user's file on the next toggle).

**No plugin host**, deliberately. When Picus wants one, do not copy sitta-be/tyto-be's
`plugin.rs` a third time — that file already carries the note to promote the host-pure wiring
into a shared `arbor-plugin-be` crate first.

#### The RPC surface served today

Everything reaches `picus-be` through `picus(method, params)` (`src/lib/ipc/rpc.ts`), except the
three password calls, which are Tauri commands straight to the shell.

| Method | Module | Notes |
|---|---|---|
| `be_ping` / `be_echo` | `selftest` | handshake |
| `get_picus_config` / `set_picus_config` | `config_cmds` | per-profile `picus/config.toml` |
| `picus_providers` | `providers` | every engine, connectable or not |
| `picus_list_connections` / `picus_save_connection` / `picus_delete_connection` | `connections` | list carries live state + `hasSecret` |
| `picus_connect` / `picus_disconnect` / `picus_test_connection` | `connections` | test opens+closes without touching the pool |
| `picus_read_db_version` | `connections` | empty string when the table isn't there — not an error |
| `picus_read_schema` / `picus_table_detail` / `picus_fetch_page` | `schema` | tree vs detail: constraints only when a tab opens |
| `picus_execute` / `picus_cancel` | `query` | cancel opens a second connection, hence it works mid-query |
| `picus_emit` / `picus_validate_rows` / `picus_validate_value` | `emit` | **served but not yet called by the frontend** |
| `picus_store_secret` / `picus_delete_secret` / `picus_has_secret` | shell command | `commands/picus_commands.rs` — never the backend |

Events: `arbor://picus-be-up` / `-down` (shell), `picus://connection-changed` (backend).

#### The password path (decided 2026-07-27)

The connection form sends the password **straight to the shell** (`picus_store_secret`), which
puts it in Arbor's keychain. It never enters the `picus-be` process at that point. When a
session is opened, `picus-be` asks for it back over the reverse channel (`__picus_secret`) and
drops it as soon as the driver has authenticated (`Secret` zeroes on drop, and its `Debug`
prints `***`).

Two details are load-bearing rather than decorative:

- **The keychain account is namespaced shell-side.** `picus-be` sends a connection *id*; the
  shell turns it into `picus/<id>` and validates it against a conservative character set. A
  backend that asked for `github.com/arbor` gets an error, not a git token. Tested.
- **"Not typed" and "cleared" are different.** The form's password is `null` until touched;
  an empty string is a deliberate delete. Collapsing them would silently destroy a saved
  credential on an unrelated edit, so the store takes `password?: string` and only writes when
  it is not `undefined`.

Driver choices (approved): `tokio-postgres` (pure Rust, no libpq to ship, and the only driver
offering a real server-side cancellation key), TLS via rustls + `rustls-native-certs` so an
internal corporate CA works with no bundle shipped and nothing links OpenSSL.

#### What `PicusConfig` covers, and what it must not

Persisted per profile (`arbor/profiles/<active>/picus/config.toml`): the encoding fallbacks,
the write guards, the emission defaults, the query row limit — the settings modal's sections,
one for one.

A **script project's** own settings — declared encoding, line ending, version table — stay in
memory for now and belong to the *project's* config, never to this file: a colleague opening
the same repository must inherit them, or the same repo behaves differently per user, which is
the class of surprise Picus exists to remove.

Two shapes worth keeping when extending it: the insertion rules are stored as **wire strings**
with typed accessors (an unknown value must degrade to the default, not fail the file's parse
and silently reset every other setting), and `row_limit` is **clamped on read** rather than
trusted (a hand-edited `0` would mean "fetch nothing" and read as a broken product).

---

## 3. Decisions taken

### Adaptations of the original brief (agreed with the user)

| Brief said | Decision |
|---|---|
| A `picus-app` Tauri crate | **No.** Arbor is Model D (1 FE + N headless BE): `picus-be` + glue in the shell |
| A `picus-fs` crate | **No.** `arbor-fs` already owns encoding-aware read/write |
| i18n, Italian as source language | **Dropped.** Arbor has no translation system; everything is English, like the rest of the suite |
| A right-hand on-demand panel | **Dropped.** Picus uses the standard Arbor layout: activity bars, left sidebar, bottom dock, status bar |
| "Reuse the shared virtualised data grid" | It did not exist — **built** it (`shared/ui/DataGrid.svelte`) |

### Product decisions

- **Highlighting** rides on CodeMirror's legacy SQL modes, picked per dialect (`plSQL` /
  `pgSQL`) — already a dependency, no new library. The *real* parse belongs to `picus-parse`
  in the backend and never to the editor.
- **Dialect colours** come from the theme's workspace ramp (`--ws-color-*`); no hex literals.
- **Connection identity is colour**, the same mechanism as Corvus workspaces: sidebar row,
  every bound tab, status bar.
- **No password in Picus.** The connection modal has no password field, by design.
  Credentials belong to Arbor's keychain; Picus asks for a handle and receives the secret at
  the moment of use.
- **No language model anywhere in the flow** — generation is structured input → model →
  per-dialect emission. Product requirement, not a preference.
- **Read-only connections are enforced in the backend**, never by hiding buttons.
- **Version table is configuration, not a constant** — `VersionTableConfig { table,
  versionColumn, dateColumn: string | null, filter }`. The date column is genuinely optional:
  with none, the closing UPDATE omits it rather than failing on a column that isn't there.
  A `detectVersionTable()` heuristic proposes it from the live schema and reports what it
  found instead of applying silently.
- **Schema ≠ project.** `schemaStore` is what a live connection reports; `picusProjectStore`
  is what is on disk. They were conflated at first and it leaked immediately (the generator
  asked the project for column types).
- **Sequences and triggers share the `table` tab kind** (`objectKind` discriminates). They
  share the frame — name, connection, sub-views — and differ only in contents.
- **Table data is paged AND virtualised.** Paging bounds what is fetched, virtualisation
  bounds what is drawn; that is why page sizes go up to 10 000.
- **Generator layout** breaks on a **container query** against the panel, not a media query
  against the viewport: the same window is wide with the sidebar closed and narrow with it
  open.
- **Insertion point of a generated block is a stated, dull rule**, shown in the diff hunk
  header. A predictable rule you can read beats a clever one you cannot.

### Gotchas already paid for

- `queryStore` originally materialised a tab's record inside a `$derived` →
  `state_unsafe_mutation`. Split into a pure `read()` and an `ensure()` called from an
  effect / event handler.
- `Record<SurfaceId, …>` maps are exhaustive: adding `picus` to `SurfaceId` without adding
  the entry to `WorkspaceTabs`' `ICONS` produced `$.get(...) is not a function` at mount.
  Both such maps are `WorkspaceTabs.ICONS` and `SurfaceHost.LOADERS`.
- A flex container with `overflow: auto` **squashes** its children instead of scrolling them.
  The document-flow views (`GenerateView`, `InventoryView`) own their scroll and set
  `flex-shrink: 0` on children; `.doc-body` only fills.

Paid for in the backend waves (2026-07-27):

- **`#[arbor_rpc::handler] async fn` works** — the `Dispatcher` collects async handlers
  separately and `block_on`s them on a serve-loop worker thread, never on a runtime worker. So
  a handler may await freely, and may also block (the `host_call` for a secret does exactly
  that) without risking the reverse-channel deadlock.
- **`tokio-postgres-rustls` must be `0.13`**, not the newest `0.14`: 0.13 is the one built
  against `rustls` 0.23, which is what the rest of the workspace resolves to.
- **A crate whose tests serialise needs `serde_json` as a `dev-dependency`.** `picus-db-postgres`
  and `picus-ast` don't use it at runtime and it is missing from the normal deps on purpose.
- **`&model().something()` in a test doesn't compile** (E0716 — the temporary dies at the end
  of the statement). Bind it: `let m = model();`.
- **A frontend store that both reads and writes config must distinguish "untouched" from
  "cleared."** The connection password is `null` until typed and `''` when deliberately
  emptied; collapsing them deletes a saved credential on an unrelated edit.

---

## 4. Directives (2026-07-27)

> §4.1, §4.2 and §4.3 are **implemented** — kept here because they are the *reasoning*, and the
> next engine will need it. §4.4 (the editors) is **not started**.

### 4.1 PostgreSQL first; Oracle scripts, not Oracle connections

The database client integrates **PostgreSQL only** to begin with. This settles the open
"Oracle driver" question by deferring it: no ODPI-C, no Instant Client, no packaging impact
for now.

**What stays fully supported for Oracle from day one is the script side** — reading, parsing,
analysing, generating and rewriting Oracle SQL files. Oracle emission (`MERGE … FROM DUAL`,
`DECLARE … BEGIN … END; /`, `USER_TABLES`, `SYSDATE`) is a pure text concern and needs no
driver. The frontend already behaves this way; the backend must too.

Practical consequence: a project can have an Oracle branch that is maintained, checked and
generated into, while no Oracle *connection* exists. The UI must not imply otherwise — a
generation target's dialect comes from its folder, never from an open connection.

### 4.2 A provider trait, one crate per engine

Mirror the `GitProvider` layout that already works for GitHub/GitLab:

```
crates/products/corvus/git-provider/{api,github,gitlab}   ← the model
crates/products/picus/db-provider/{api,postgres,oracle}   ← to build
```

- `picus-db-api` — the trait plus the shared types: connect / disconnect, execute, cancel,
  read schema (tables, views, sequences, triggers, columns, FKs, indexes), fetch a page,
  describe capabilities. Nothing engine-specific.
- `picus-db-postgres` — the first (and initially only) implementation.
- `picus-db-oracle` — a later crate, added without touching anything above it.

Follow `git-provider/api`'s file split as the template: `provider.rs` (the trait),
`capability.rs`, `error.rs`, `registry.rs`, `kind.rs`, `prelude.rs`. Same rule as everywhere
else in the workspace: the crate's public API goes through `pub mod prelude`.

The registry matters as much as the trait: adding an engine should be registering an
implementation, not editing a `match` in five places.

### 4.3 Data-driven per-engine UI

The engine changes what the interface should show — most visibly in the **create-connection
modal**, but not only there. Today the frontend decides this with `if (dialect === 'oracle')`
scattered across components. That has to become **data served by the backend**, the same way
`corvus/provider-descriptor` already does it for git providers.

A `DbProviderDescriptor` should carry at least:

- **connection fields** — id, label, kind (text / number / secret-handle / select), default,
  placeholder, required. Oracle wants *service name*; PostgreSQL wants *database*; the
  default port differs; Oracle may want a TNS-alias mode that PostgreSQL has no concept of.
- **capabilities** — does the engine have sequences? bitmap or function-based indexes?
  materialised views? schemas vs databases? Drives which groups the schema tree shows and
  which structure sections exist.
- **emission traits** — block delimiter, upsert form, current-date function,
  object-existence check, transaction semantics, identifier casing. These are *already*
  encoded, but as branches inside the emitter; long-term they belong in the descriptor so a
  third engine is data plus a crate, not edits to a `match`.
- **labels** — display name, short chip label, colour token.

Migration targets — where the hardcoded branching lives today:

| File | What is hardcoded |
|---|---|
| `types/picus/index.ts` → `DIALECTS` | labels, short names, colour tokens |
| `PicusConnectionModal.svelte` | service-vs-database label, default port, placeholders |
| `ipc/picus/mock-emit.ts` | every dialect difference in emission |
| `picus-sql-language.ts` | which CodeMirror mode to use |
| `TargetEditor.svelte` | the "what this rule becomes" hints (`USER_TABLES` vs `to_regclass`) |
| `ConnectionsPanel.svelte` | identifier casing on display |

None of this is urgent while there are two engines, but the descriptor should exist **before
the third**, and the trait split (§4.2) should be done from the start so the descriptor has
somewhere to come from.

### 4.4 The SQL editors must feel like Bennu's

Picus's editors (query tabs and script files) are not textareas with colours. They have to
carry the same weight as Bennu's Java editor: **ghost text, autocompletion, hover, live
diagnostics, navigation**. Someone who writes Java in Bennu and SQL in Picus should not feel
they changed tool.

#### What is already generalised — most of it

`shared/ui/code-editor/` is genuinely product-agnostic and driven by a
`LanguageDescriptor`. Available today **without touching Bennu**:

| Capability | How Picus gets it |
|---|---|
| Autocompletion | `descriptor.intel.completion` — a CodeMirror `CompletionSource`; the core installs `autocompletion({override})` + keymap |
| Hover docs | `descriptor.intel.hover` — a `hoverTooltip` source (350 ms), async allowed |
| Diagnostics | `diagnostics` prop, `EditorDiagnostic[]` in **UTF-8 byte offsets**, mapped to CM ranges by the core; lint gutter included |
| Folding, bracket matching, auto-close, selection-match, search panel | on by default |
| Minimap, sticky scroll, indent guides, rainbow brackets, scrollbar overview, ruler | opt-in props |
| Toggle comment (`Ctrl+/`) | `descriptor.commentTokens` |
| Go-to inside the buffer | `descriptor.resolveGoto` (tree-sitter descriptors) or the `onGoto` callback (host decides) |
| Embedded languages | `descriptor.injections` |

So the plumbing exists. What Picus has to write is the **SQL intelligence behind those
hooks**, not the editor.

#### What does NOT exist anywhere in Arbor: ghost text

There is **no inline-completion / ghost-text extension** in the codebase — Bennu doesn't have
one either, so there is nothing to generalise: it has to be **built in the shared core**, as
a new `intel` hook, and Bennu should adopt it afterwards. Rough shape:

```ts
// types.ts → CodeEditorIntel
/** Inline continuation shown greyed at the caret; Tab accepts, Esc dismisses. */
inlineCompletion?: (view: EditorView, pos: number) => string | null | Promise<string | null>;
```

plus a `ViewPlugin` in `extensions.ts` drawing it as a widget decoration, and its keymap. It
belongs to the shared core, not to Picus: the moment it exists, Bennu wants it too.

> **Hard constraint.** In most editors "ghost text" means an AI completion. In Picus it must
> **not** be: the product forbids language models anywhere in the flow (§3). Picus's ghost
> text is **deterministic and schema-derived** — after `INSERT INTO PARAMETRI (` the column
> list is not a guess, it is a fact the connection already told us. That is a stronger
> feature than a prediction, and it must be built and described as such.

#### What the SQL intelligence has to know

Everything below comes from the schema (`schemaStore` / the backend), never from a model:

- **Completion**: tables, views, sequences, columns, and dialect keywords. Context-aware and
  **alias-aware** — after `FROM CLIENTI c … WHERE c.` the candidates are `CLIENTI`'s columns,
  not every column in the schema. In the DML generator's paste box, the same source.
- **Ghost text**: the deterministic continuations — the column list after
  `INSERT INTO <table> (`, the matching `VALUES (…)` skeleton, the join predicate implied by
  a foreign key after `JOIN ORDINI o ON `, the closing `END;` / `END $$;` of the block being
  opened.
- **Hover**: a column's type, nullability, default and FK target; a table's row estimate; a
  sequence's last value. All facts already in the schema snapshot.
- **Diagnostics while typing**: unknown table or column, ambiguous unqualified column,
  writing on a read-only connection — *before* the statement runs, not after it fails.
- **Navigation**: from a table name in a script to that table's tab, and from a table name in
  a query to where the repository defines it (the inventory already maps object → files).

#### One caveat about the current language descriptor

`picus-sql-language.ts` uses `cmExtension` (CodeMirror's legacy `plSQL` / `pgSQL` stream
modes). With `cmExtension` the **tree-driven** features are inactive — `resolveGoto` and
`foldNode` need a live tree. `intel.completion` / `intel.hover` / diagnostics do **not** care
and work as-is.

So the order is: intel hooks first (they pay off immediately and need no grammar), and a real
tree-sitter SQL descriptor only when in-buffer navigation and structural folding are wanted —
which is the same grammar decision as §6.5, and should be made once for both.

---

## 5. What is left to do

### Start here

Two pieces, in this order.

**1. Wire the generator to the backend emitter.** `picus-emit` is done and covered by 29 golden
tests; `picus_emit` / `picus_validate_rows` / `picus_validate_value` are served; nothing calls
them. The work is `stores/picus/dml.svelte.ts` plus `generate/{DmlValueGrid,SqlPreview,CsvImportGrid}.svelte`,
then deleting `ipc/picus/mock-emit.ts`.

Two things to carry across rather than re-invent: the model the frontend builds must map onto
`picus_ast::DmlModel` (snake_case on the wire, `dateColumn: null` meaning "this project stamps
no date"), and `parsePastedInserts` / `parseCsv` / `proposeCsvMapping` are still frontend-only —
they belong to `picus-parse` later, so leave them where they are rather than half-moving them.

**2. `picus-parse`** — full Tree-sitter grammar (decision §6.5), Arbor's own pattern: own
grammar, generated `parser.c` committed, no Node at build time, as Merula does. Explicit user
direction: **a very large body of unit tests over every kind of SQL that can be thought of**,
with permission already given to run the `tree-sitter` generation commands.


### Backend

1. ~~`picus-core` — `PicusState` + prelude.~~ **Done.**
2. ~~`picus-be` — `[[bin]]`, slim `main.rs`, `be_ping` + the typed config.~~ **Done.**
3. ~~Workspace `members`, `router.register`, `ensure_picus_be` / `spawn_picus_be`,
   `open_picus_window` on `spawn_blocking`.~~ **Done.**
4. ~~`picus-db-api` + `picus-db-postgres` (§4.2), the registry and pool on `PicusState`,
   read-only enforced in the backend.~~ **Done.** Read-only is enforced by the *server*:
   the session is opened `AS TRANSACTION READ ONLY`, so the refusal holds for a pasted script
   too; the lexical check only makes the message better and arrive sooner.
5. ~~`picus-ast` + `picus-emit`~~ **Done**, with `picus-types` extracted under both halves.
   Emission is in Rust with 29 golden tests; `picus_emit` / `picus_validate_rows` /
   `picus_validate_value` are served. `picus-emit` knows `picus-ast` and **never**
   Tree-sitter — keep it that way.
6. `picus-parse`, `picus-inventory`, `picus-analyze`, `picus-rewrite` — the rest of the script
   half, and **the next step**. Created one at a time as each is activated (decision §6.2), not
   all up front.
6. Encoding: extend the detection in `arbor-fs` (BOM → UTF-8-with-multibyte → ASCII-neutral
   inherited from the folder → single-byte heuristic). Open question: whether to extend
   `arbor-fs` in place — which changes behaviour for Bennu and Corvus — or layer it.

### Frontend, as each backend domain lands

- ~~Connections, schema, table rows and query execution on real RPC.~~ **Done** — the
  connection modal now reads its fields, labels, defaults and capabilities from
  `picus_providers` rather than branching on the engine, and an engine with no driver says so
  instead of disappearing. `mock.ts` is down to the script-side fixtures plus
  `DEFAULT_QUERY_TEXT`; `mock-emit.ts` is untouched and goes with `picus-emit`.
- Replace the remaining `ipc/picus/mock.ts` (project tree, inventory, findings) and
  `mock-emit.ts` with real RPC as the script crates land.
- ~~Settings persist to `…/picus/config.toml` via `get_picus_config` / `set_picus_config`.~~
  **Done** for the product settings (never `localStorage`).
- Project-level settings (encoding, EOL, version table) are still in memory: they belong in the
  project's own config so a colleague opening the same repository inherits them. They persist
  when the script half gives the backend a project to attach them to.

### Editor intelligence (§4.4)

1. **Ghost text in the shared core** — `intel.inlineCompletion` hook + a `ViewPlugin` widget
   decoration + keymap in `shared/ui/code-editor/`. Nothing to generalise from Bennu: it does
   not exist there either. Bennu adopts it once it lands.
2. **A SQL completion source** — tables / views / sequences / columns / keywords, alias-aware,
   fed by `schemaStore` (later by the backend). Wired through `descriptor.intel.completion`.
3. **A SQL hover source** — column type, nullability, default, FK target; table row estimate.
4. **Live diagnostics** — unknown table or column, ambiguous unqualified column, writes on a
   read-only connection. Byte offsets in, the core maps them.
5. **Deterministic ghost-text rules** — column list after `INSERT INTO t (`, the `VALUES`
   skeleton, FK-implied join predicates, block closers. Schema-derived, never predicted.

### Known frontend debt

- `PicusToolbar.svelte` branches by tab kind and is getting long — split into one toolbar per
  document type before it becomes the place where "which button applies to what" hides.
- `CreateWorkspaceModal` / `GroupFormModal` (Corvus) can now use `ColorPalettePicker`, and
  `TytoShortcutsModal` can use `ShortcutsReference`. Both are a few lines; not done to avoid
  touching working products unasked.
- `shared/ui/Tree.svelte` still uses native HTML5 drag-and-drop, which WebView2 drops.

### Tests

29 unit tests, all green, none needing a database:

- `picus-core` — config round-trip, a partial file keeping its siblings' defaults, an unknown
  insertion rule degrading instead of failing the parse, the row limit clamped, and the one
  that matters most: **`connections.toml` never contains a secret**.
- `picus-db-api` — the wire shapes the frontend reads (`type`, `primaryKey`, `estimatedRows`
  absent rather than `0`), `null` surviving a round-trip distinct from `""`, `Secret`'s `Debug`
  refusing to print the value.
- `picus-db-postgres` — identifier quoting neutralising a hostile table name, statement
  classification (leading comments, a data-modifying CTE, `UPDATED_AT` not counting as
  `UPDATE`), trigger bitmask decoding, and the value mapping (`007` staying text, a decimal
  too precise for `f64` staying text).
- `arbor` — the keychain namespace refusing every id that could escape `picus/`.

`.config/nextest.toml` now exists with the mandatory per-test timeout (a loop in a parser must
fail, not hang CI): `cargo nextest run`. Everything below is still owed:
- The **byte-identical round-trip** over a corpus of real files is the most important test of
  the project.
- Golden tests per operation × dialect × rules.
- Encoding: windows-1252 with accents, UTF-8, BOM, ASCII-only, and a character not
  representable in the destination (must fail cleanly).
- Transactional apply: fail on the n-th file, verify the previous ones rolled back.

---

## 6. Still open

1. **`NamingScheme`** for update scripts — versioned (`4_12__4_13.sql`) vs dated. The real
   questions are (a) where the "next version" comes from (the connected database, the highest
   file on disk, or typed by hand) and (b) what happens to `VER003` (unbroken version chain)
   under the dated scheme, where it arguably has no meaning. **Needed before `picus-rewrite`
   writes its first file** — not before then.
2. ~~**Crate granularity.**~~ **Decided (2026-07-27):** create each crate as it is activated,
   the way Bennu and Merula grew. No empty scaffolds.
3. **Encoding detection placement** — extend `arbor-fs` (shared benefit, changes existing
   products' behaviour) or layer it for Picus only.
4. **WASM tier** — `types` / `ast` / `emit` are wasm-clean **today** (serde only, no I/O),
   which is exactly the slice a "generate SQL" plugin would need; `analyze` / `inventory`
   should stay that way. `parse` is the awkward one: the Tree-sitter runtime needs a C
   toolchain for wasm32, so it should sit behind a feature.
5. ~~**Tree-sitter PL/SQL coverage.**~~ **Decided (2026-07-27):** a full Tree-sitter grammar,
   Arbor's own pattern — own grammar, generated `parser.c` committed, no Node at build time,
   as Merula does. It is the long road and the honest one; no mature PL/SQL grammar exists to
   borrow. Explicit direction from the user: **a very large body of unit tests over every kind
   of SQL that can be thought of**, and permission to run the `tree-sitter` generation
   commands.

### Decisions taken on 2026-07-27 (this session)

| Question | Decision |
|---|---|
| PostgreSQL driver | `tokio-postgres` — pure Rust, no libpq, real server-side cancel key |
| TLS | rustls + `rustls-native-certs` (OS trust store, no OpenSSL linked) |
| Where the password lives | Arbor's keychain, written by the shell, read by the BE over the reverse channel |
| Scope of the DB step | backend **and** frontend wired, not backend alone |
| Shared `Column` / `EngineKind` | extracted into `picus-types`, a leaf under both halves |
| Crate granularity | one at a time, as activated |
| Generated SQL identifiers | **English** (`before_changes`, `v_version`, `v_existing`, `v_object`) |
| Parsing strategy | full Tree-sitter grammar, with a very large test suite |
