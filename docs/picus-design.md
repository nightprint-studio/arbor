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

### Frontend — complete, running on fixtures

~50 files under `src/lib/{components,stores,ipc,types}/picus/`. Precedent followed: **Tyto**
(mocked UI + window wiring landed first, backend after).

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

### Backend — does not exist

No `picus-be`, no crates. Hook points are already commented in place:

- `src-tauri/src/window/picus.rs::open_picus_window` — where `ensure_picus_be` goes, on
  `spawn_blocking` (the blocking-pool rule is mandatory, see `docs/backend-architecture.md`);
- `src-tauri/src/window/workspace.rs::ensure_backend_for` — the `match` arm for tabbed mode.

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

---

## 4. New directives (2026-07-27) — not yet implemented

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

### Backend (nothing exists yet)

1. `picus-core` — `PicusState` + prelude (model on `TytoState`).
2. `picus-be` — `[[bin]]`, `main.rs` from the `sitta-be` skeleton, `be_ping` first.
3. Register both in the workspace `members`; `router.register("picus", SplitBroker::pure_oop("picus"))`;
   `ensure_picus_be` + `spawn_picus_be`; call it from `open_picus_window` on `spawn_blocking`.
4. `picus-db-api` + `picus-db-postgres` (§4.2).
5. `picus-ast`, `picus-parse`, `picus-inventory`, `picus-analyze`, `picus-emit`,
   `picus-rewrite` — the script half. `picus-emit`/`picus-analyze` must know `picus-ast` and
   **never** Tree-sitter.
6. Encoding: extend the detection in `arbor-fs` (BOM → UTF-8-with-multibyte → ASCII-neutral
   inherited from the folder → single-byte heuristic). Open question: whether to extend
   `arbor-fs` in place — which changes behaviour for Bennu and Corvus — or layer it.

### Frontend, once the backend exists

- Replace `ipc/picus/mock.ts` and `mock-emit.ts` with real RPC (`picus(method, params)`).
- Settings must persist to `…/picus/config.toml` in the active profile via
  `get_picus_config` / `set_picus_config`. They are in-memory today. **Never `localStorage`.**
- Project-level settings (encoding, version table) belong in the project's own config so a
  colleague opening the same repository inherits them.

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

Nothing yet — there is no Rust to test. When the crates land:

- `cargo-nextest` with a per-test timeout is **mandatory** (a loop in a parser or a rewriter
  must fail, not hang CI). No `.config/nextest.toml` exists in the repo today.
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
   under the dated scheme, where it arguably has no meaning.
2. **Crate granularity** — all ten crates now, or `core` + `be` and split as each is
   activated (which is how Bennu and Merula actually grew).
3. **Encoding detection placement** — extend `arbor-fs` (shared benefit, changes existing
   products' behaviour) or layer it for Picus only.
4. **WASM tier** — `ast` / `emit` / `analyze` / `inventory` can be wasm-clean, which is
   exactly the slice a "generate SQL" plugin would need. `parse` is the awkward one: the
   Tree-sitter runtime needs a C toolchain for wasm32, so it should sit behind a feature.
5. **Tree-sitter PL/SQL coverage** — the biggest technical risk. No mature grammar exists;
   Arbor's own pattern (own grammar, generated `parser.c` committed, no Node at build time)
   is the fallback. Measure the percentage of statements landing in `Other` on a real corpus
   **before** committing to the parsing milestone. A plausible outcome: for Picus's actual
   scope (DML, anonymous blocks, guards, routines treated as opaque), a robust statement
   splitter plus a targeted grammar beats full PL/SQL.
