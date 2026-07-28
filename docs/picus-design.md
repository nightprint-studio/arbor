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

**The dialect is a property of the FOLDER, never a global "current dialect".** Every function
that generates, analyses or rewrites SQL takes it as an explicit parameter. There is no ambient
dialect anywhere in the design, and adding one would break the product's single reason to exist.

> **Corrected on 2026-07-28, and worth reading before touching the tree.** The implementation
> first translated "the folder" into "the *top-level* folder": discovery assumed level 1 was a
> branch carrying a dialect, level 2 a folder carrying a role, and flattened everything below.
> A real repository proved it wrong — `AGGIORNAMENTO/…/ORA` and `…/POS` put the **dialect at the
> bottom and the role at the top**, and the panel showed a flat list of eleven identical `ORA`
> rows. `Branch` is gone. The tree is the real directory hierarchy; a folder may *declare* a
> dialect and/or a role, and descendants **inherit** the nearest declaration until one overrides
> it. That is the literal reading of the invariant, and the one the product needed all along.

Three corollaries the code honours:

- one generation produces N files, each correct on its own terms;
- a rule that makes sense for one role (a version guard on an update script) must never
  propagate to another (an initialisation script);
- a folder nobody could classify has **no** dialect, and nothing is generated into it. Never a
  default — guessing here writes Oracle syntax into a PostgreSQL file, which is the failure the
  whole product exists to catch. The interface asks instead.

> **Extended on 2026-07-28: four engine states, not one and a hole.** "No dialect" was doing the
> work of three different facts. `picus_types::FolderEngine` is now the one slot a folder's
> engine lives in, and it has all four answers:
>
> | | Means | Behaviour |
> |---|---|---|
> | `Supported(EngineKind)` | Oracle / PostgreSQL | parsed, analysed, compared, generated into |
> | `Generic` | **portable** SQL, valid on both | parsed against both, counts for both, generated into with the intersection |
> | `Unsupported(ForeignEngine)` | recognised, unsupported | named, **never asked about, never parsed** |
> | *(absent)* | nobody knows | the interface asks |
>
> **Unsupported** exists because `AGGIORNAMENTO/2024/MSQ` is SQL Server and `…/DB` is DB2, and
> asking about them forever is how a panel teaches people to stop reading it. Never being able
> to answer a question is a different fact from not knowing the answer.
>
> **Generic** exists because the same repository has folders of plain `INSERT`/`UPDATE`/`DELETE`
> meant to run on *both* engines, and declaring them Oracle or PostgreSQL was a lie either way —
> one whose cost was that the engine they were *not* got reported as missing everything they
> contained. It is **never inferred**: a promise of portability is something a person makes.
>
> The mechanism that carries all four is `FolderEngine::scope() -> Option<DialectScope>`, where
> `DialectScope` is `One(EngineKind) | Portable` — deliberately with no unsupported and no
> unknown member, so a parse or generation target in such a folder is *unrepresentable*. Two
> dual predicates hang off it and the whole feature lives in the gap between them:
> `covers(dialect)` (does content here count for that engine — **true of both** under
> `Portable`) and `permits_syntax_of(dialect)` (may syntax specific to it appear — **false for
> both** under `Portable`). The first puts a portable folder in every lane; the second inverts
> `DIA001`. `FolderNode` carries one `engine` field and one `effective_engine` field, read
> through `scope()` / `covers()` / `effective_dialect()`.

> **Corrected again on 2026-07-28: the engine is a property of the FILE, of which the folder is
> the default.** The invariant as first written was right about where the answer usually lives and
> wrong about where it *can* live. A tidy repository puts the engine on a directory and everything
> in it inherits — still the case for essentially every file in essentially every repository. An
> untidy one puts it in the file name: `4_12_ORA.sql` beside `4_12_POS.sql` in one folder that can
> say nothing about either, because it is honestly both. There was nowhere to write that down, so
> the folder stayed unclassified and everything downstream went quiet about half the repository —
> no lane, no coverage column, no cross-dialect comparison, and a "no engine" warning that no
> answer could remove.
>
> The chain therefore gained one link at the bottom: **file declaration → folder declaration →
> nearest ancestor's → none**. `ScriptFile` carries `engine` (declared on this file) and
> `effective_engine` (after inheritance), read through exactly the same
> `scope()` / `covers()` / `effective_dialect()` methods `FolderNode` exposes — the same four
> answers, at one more level of granularity, so a file and its folder can never disagree about what
> *portable* means. `resolve()` fills both in one pass, because one inheritance rule in the
> codebase is the point.
>
> What now asks the **file** rather than the folder: which dialect a script is parsed as, whether
> it is parsed at all, which lane it counts in, and which dialects the repository is taken to have.
> What still asks the **folder**: the role. A role is what a *directory of scripts* is for, and the
> file beside this one in the same directory is for the same thing; the engine is the one axis that
> genuinely varies file by file. Three corollaries fall out:
>
> - **The coverage column splits.** A column is keyed on the folder's path, which in a mixed folder
>   would add the Oracle statements to the PostgreSQL ones and destroy the only comparison the
>   table exists to make. Such a folder yields one column *per engine* instead, with the engine in
>   the header — `AGG · Oracle`, `AGG · PostgreSQL`, `AGG · unclassified`. A tidy folder, which is
>   every folder of every repository that existed before this, is spelled byte-identically to
>   before. `Placement::coverage_key` and `ParsedProject::coverage_keys` walk that rule through
>   **one** function: two implementations would give the table a column nothing counts towards and
>   lose one that does.
> - **A folder is in a lane when any file in it is** — so a directory holding one `*_ORA.sql` and
>   one `*_POS.sql` is in both lanes, where before it was in neither. A folder holding only other
>   folders is in none, which is right: it has no content to compare.
> - **The cross-dialect rules ask the sites, not the coverage map.** In a mixed folder the coverage
>   column is the folder's, so summing it into each lane would credit Oracle with what the
>   PostgreSQL scripts did — a false negative, the one kind of wrong answer `CONS001` must never
>   give. `lane_touches` / `lane_statements` replaced `coverage_of` for exactly that reason.

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
| Generator (form / paste / CSV / preview / diff) | **Real emission.** `stores/picus/dml.svelte.ts` → `picus_emit` / `picus_validate_rows`, debounced and cached. Reading a *paste* is the one piece still frontend-side (`utils/picus/paste-sql.ts`) until `picus-parse` serves it. |
| Script tree, inventory, consistency findings, file text | **Real.** `picus_open_scripts` / `picus_analyze_scripts` / `picus_script_text`, driven by `picusProjectStore`. A repository belongs to a **connection** (`ConnectionSpec.scriptRoot`): you open a database and its scripts are what you see. |
| Writing generated SQL into files | **Real, and two-step.** `picus_preview_apply` returns the exact bytes; `picus_apply` re-plans through the *same* code path and refuses if a file changed since the preview. |

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
| Stores | `stores/picus/{ui,connections,schema,project,tabs,dml,query,result,consistency,settings}.svelte.ts` |
| Types | `types/picus/index.ts` |

### Widgets contributed to the shared library

| Widget | Why it is shared |
|---|---|
| `shared/ui/DataGrid.svelte` | Virtualised, sortable, filterable, resizable. NULL ≠ empty string; numbers right-aligned with tabular figures. Takes either a plain array or a `DataGridWindowSource` — a total, a `rowAt(i)` and a `request(start, count)` — so a grid can be a window onto something far larger than memory, with no Arbor concept inside the widget |
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

- ~~`src/lib/ipc/picus/mock.ts`~~ — **deleted.** Nothing in Picus reads a fixture any more.
- `src/lib/utils/picus/paste-sql.ts` — a regex reader for pasted INSERTs, all that survives of
  the emitter stand-in. It is the one piece of that file that genuinely *was* SQL parsing, so
  it goes the moment `picus-parse` serves a `picus_parse_inserts` handler. Its neighbours are
  permanent: `csv.ts` is file handling, `sql-values.ts` is a display marker.

### Backend — the database half is live

`picus-be` runs and talks to PostgreSQL. Serving today: the typed product config, the
per-engine descriptors, connections, schema, statement execution over **held results** —
a server-side cursor whose windows are fetched as the user scrolls — and server-side
cancellation. The script half lands in the following waves against the same `PicusState`.

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
| `picus_read_schema` / `picus_table_detail` | `schema` | tree vs detail: constraints only when a tab opens |
| `picus_execute` / `picus_open_relation` | `query` | one door for every statement: a read opens a held cursor and returns its first window, a write returns `affected` and holds nothing. Opening a relation is the same path — a table tab is a read, not a mechanism of its own |
| `picus_result_window` / `picus_count_result` / `picus_close_result` | `query` | any offset, forwards or backwards; the count rewinds the *same* cursor rather than re-running the query; closing is idempotent |
| `picus_cancel` | `query` | opens a second connection, so it works mid-query — and records the run ordinal, so a cancel landing between round trips is honoured instead of lost |
| `picus_emit` / `picus_validate_rows` / `picus_validate_value` | `emit` | **served but not yet called by the frontend** |
| `picus_open_project` / `picus_confirm_project` / `picus_is_project` / `picus_propose_update_file` | `project` | discovery proposes; nothing is written before the confirmation. `confirm` edits **paths** |
| `picus_set_folder_alias` / `picus_folders_named` | `project` | the other half of classifying: edits a **name**, so it answers for every folder called `POS` including the ones that do not exist yet. `applies_to` says whether the name is looked for in folder names, file names or both. `folders_named` is what lets the offer state its blast radius before the user agrees |
| `picus_set_file_engine` | `project` | the leaf of the same chain: the engine of **one file**, for the directory holding `4_12_ORA.sql` beside `4_12_POS.sql`. No dialect clears it and the file inherits its folder again; a path the tree does not know is refused rather than written |
| `picus_open_scripts` / `picus_refresh_scripts` | `scripts` | reads, decodes and holds the whole repository; same reply shape as `picus_open_project` |
| `picus_analyze_scripts` | `scripts` | inventory + the fourteen rules + skipped rules + rejected suppressions, all in the crates' own wire types |
| `picus_script_text` | `scripts` | one file's decoded text, its encoding and its line ending |
| `picus_preview_apply` / `picus_apply` | `apply` | the preview returns the exact bytes plus a digest per file; the apply refuses if any digest moved |
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

#### The project's own config (decided 2026-07-27)

Everything that describes *a repository of scripts* rather than *this user's preferences* lives
with the repository, at **`<root>/.arbor/picus/project.toml`** — inside the existing `.arbor/`
directory, namespaced per product. (The user intends to move Corvus's per-repo config to
`.arbor/corvus/` on the same principle eventually; Picus is the first to use the shape, so get
it right here.)

What it holds: a **flat list of declarations keyed by folder path** — a dialect, a role, an
encoding, a naming scheme, any of them, on any folder — plus the version table, the update-file
naming default and the generated-block marker template.

Flat and by path, rather than the nested branch/folder arrays it started as, for a concrete
reason: adding a subdirectory used to invalidate the shape, and now it does not. A declaration
is written only where it says something the folder would not inherit anyway, so a repository
that agrees with the inference writes almost nothing. A `version = 1` file is migrated on read
rather than rejected, and the migration reproduces v1's semantics exactly through the
inheritance rule.

A folder's `dialect` key may name an engine Picus does **not** read (`dialect = "sqlserver"`):
a folder has one engine, so it has one key.

##### …and, where a repository is untidy, keyed by file

The same flat shape, one level down, for the repositories where the engine is on the file:

```toml
[[file]]
path = "AGGIORNAMENTO/2024/4_12_POS.sql"
dialect = "postgres"
```

Only the engine, and deliberately only the engine — a role is a fact about a directory, and an
encoding is measured from the bytes rather than declared. Nothing inherits *downwards* from it: a
file has nothing below it. It is a leaf answer and it beats everything, including a declaration on
the folder it sits in, which is the same "a specific answer beats a general rule" that already
orders `[[folder]]` above `[[alias]]`. Almost always empty: a `[[file]]` line is a correction to a
file Picus placed wrongly, and discovery never proposes one.

##### The version number is derived from the content, not from the build

The file is stamped with **the lowest schema version that can read it correctly** — `3` exactly
when something in it classifies an individual file (a `[[file]]` declaration, or an alias pointed
at file names), `2` otherwise. Not with `CURRENT_VERSION`, and the reason is that a version number
is a claim about compatibility while the project file is committed and shared. Stamping every save
with the newest number would lock a colleague on an older build out of a repository that uses
nothing their build lacks; stamping every save with the oldest would let that build silently ignore
the declarations deciding which dialect a script is parsed as — and silently ignoring a
classification is the failure this product exists to prevent. So the claim is computed from what
the file actually says.

#### Per-project inference aliases (decided 2026-07-28)

The same file also carries a **vocabulary** — folder *names* that mean something in this
repository:

```toml
[[alias]]
name = "POS"
engine = "postgres"

[[alias]]
name = "MSQ"
engine = "sqlserver"

[[alias]]
name = "CONSEGNE"
role = "update"
```

The problem it solves is not "Picus's keyword list is too short". It is that a declaration
answers for one *path*, and the repository this product was built for ships a folder set per
delivered version: eleven folders called `POS`, eleven `ORA`, eleven `MSQ`, eleven `DB`, and
another set next release. Declaring folder by folder does not scale to that and never will. One
alias covers all eleven **and every one added later**, which is the only property that makes the
repository describable once instead of every release.

Why per-project and not global: `pos` is far too generic — point-of-sale, `POSIZIONI` — and `DB`
means nothing at all. Adding either to the built-in list would misclassify folders in other
people's repositories, which is precisely the failure mode the whole-word tightening of the
dialect keywords was for. The built-in vocabulary is a **global heuristic** and can only hold
names that mean one thing everywhere; an alias is a **local fact its owner knows**. So the
built-in list only gained the unsupported engines' real names (`sqlserver`, `mssql`, `tsql`,
`db2`, `mysql`, `mariadb`, `sqlite`) — a folder called `DB2` is DB2 in every repository on earth
— and the abbreviations stay per-project.

Four properties, each of them a trap avoided:

- It **adds to** the built-in vocabulary. Declaring one alias must not cost a repository the
  defaults it was already relying on.
- It matches **exactly the way a built-in keyword does**: whole word, case-insensitively,
  through `alias::name_matches`, which is also what the "how many folders would this affect"
  count goes through. A second implementation of that rule would be a second one that drifts.
- It covers **roles** as well as engines — a repository whose update folder is called
  `CONSEGNE` has the same problem one axis over.
- A bad value **degrades**: `engine` and `role` are wire strings read through typed accessors,
  exactly like `[generation.insertion]`, so a typo costs that one entry and is reported by
  `ProjectConfig::problems` instead of failing the parse and resetting the file.

**Precedence: `[[folder]]` beats `[[alias]]` beats the built-in vocabulary.** A specific answer
beats a general rule; a local fact beats a global heuristic. Including the awkward corner — a
declaration that *clears* an engine is authoritative and is not re-inferred from the alias on
the next scan, exactly as it is not re-inferred from the keyword list. Aliases apply at
**discovery**, so a `POS` folder created next month is classified without anyone touching the
file.

##### Where the name is looked for: `applies_to`, and why file names are opt-in

An alias matches folder names by default. A repository whose engine is in the file name points the
same name at file names too:

```toml
[[alias]]
name = "POS"
engine = "postgres"
applies_to = "both"      # "folders" (the default) | "files" | "both"
```

An alias written without it means folders, which is what every alias written before this existed
already meant. The scope moves the **engine** only: a role is a fact about a directory whatever the
scope says, so an alias that declares nothing but a role and points at file names classifies
nothing — and is reported as such, because it is two correct-looking lines that together do
nothing. A typo in `applies_to` degrades to the default rather than to nothing: it says *where* to
look, and an unreadable one must not un-declare an engine that was spelled correctly.

**The built-in vocabulary never classifies a file**, and this is the asymmetry the whole path rests
on. Not timidity — two distinct reasons:

- **A file name is a sentence.** `ORA` is Italian for *now*: `AGGIORNA_ORA_INIZIO.sql` is an
  ordinary name for an ordinary script, and a global rule reading it as Oracle would parse
  somebody's PostgreSQL file with the wrong dialect. Folder names are short, deliberate and a dozen
  to a repository, so they get reviewed; file names are hundreds, and nobody reviews them.
- **The full product names are no safer here.** `MIGRAZIONE_DA_MYSQL.sql` is a PostgreSQL script
  *about* MySQL, and reading `mysql` out of it would mark the file as an engine Picus does not
  support — which does not produce a wrong finding, it produces **no** findings, silently.
  `4_12_ORACLE.sql` is the case this gives up, and one `[[alias]]` line buys it back.

So a file is classified by name only where the repository's owner has said which names mean what
*and* said they mean it about file names. Everywhere else the file simply inherits its folder. The
extension is taken off before matching, so `.sql` can never match an alias called `SQL` and a
repository whose Oracle files end in `.pks` is not classified by that accident either.

Where they come from and where they are reviewed: classifying a folder whose name repeats raises
a **second, distinct** confirmation offering to make it a rule, naming the count before the user
agrees (`picus_folders_named`); and the accumulated vocabulary is listed, editable and removable
under Settings ▸ Project ▸ **Folder names**, with each row's current reach shown.

Two rules about how it comes into existence:

- **It is proposed, shown, and only written after an explicit confirmation.** Picus infers the
  whole thing from the tree — folder names, the dominant encoding per folder, the shape of the
  update filenames — and presents it. Nothing reaches the disk until the user agrees. This is
  the app-wide rule (no automatic writes) and it matters more than usual here, because the file
  lands in someone's repository and gets committed.
- **It is the reason these settings are not in `PicusConfig`.** A colleague opening the same
  repository must inherit them, or the same repo behaves differently per user — which is the
  class of surprise Picus exists to remove.

#### The generated-block marker (decided 2026-07-27)

A block Picus writes into a file is **marked, and the marker is a configurable template**. The
default sits in the `-- picus:` namespace already used by suppressions, and the template accepts
placeholders (`{from_version}`, `{to_version}`, `{table}`, `{hash}`) because projects want
different things in that line — several want the version transition spelled out on every block.

The marker is what makes an apply **idempotent**: Picus can recognise a block it wrote and
regenerate it in place instead of appending a second copy. A project that wants its files free
of tool markers can empty the template, and loses exactly that.

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
- **Table data is one continuous scroll, like a query result.** There is no page selector
  and no page size; the row total lives in the status bar. Opening a relation is a *read*
  that goes down the same path as a typed statement, holds the same kind of result and is
  closed the same way — making it a mechanism of its own is how a product ends up with two
  scrolling behaviours in two panels that both show rows, which is exactly what made this
  one feel inconsistent. Windows arrive before the viewport reaches them: the grid asks for
  the first *gap* in a band widened by a margin (a fifth of the window, so "start fetching
  around row 400 of 500" scales instead of being a constant), and deriving the request from
  the gap rather than from the scroll position is what makes a repeated ask harmless.
  A corollary paid for on a 695-table database: `table_detail` pins its query to the one
  relation, because reading all of a several-hundred-relation schema to find one returns a
  row per *column* of every one of them.
- **Nothing is capped, so what must be said out loud changed.** The row limit is no longer a
  ceiling — it is the number of rows in one window — and a result is never truncated, which
  removes the correctness bug the cap had (a cut tail is indistinguishable from an end). Two
  things take its place, and both are about not implying precision Picus does not have:
  - **The length is an estimate until it isn't.** The scrollbar is scaled immediately by the
    planner's guess and every total is written `~` until a background `picus_count_result`
    replaces it. That count rewinds the *same* cursor rather than running `count(*)`, which
    would re-execute the query and so answer about a different moment — a total disagreeing
    with the result being scrolled gives the grid a length that is simply false, and it then
    asks for rows that are not there.
  - **Sorting and the per-column filters are disabled while the result is partial**, visibly,
    with the reason stated — not hidden, because a missing control reads as "this grid does
    not sort", a different and untrue statement. They were already acting on fetched rows
    only under the cap; with a window open that silence would have become a bigger lie. Both
    return the moment the whole result is loaded.
- **Cancellation is remembered, not merely sent.** `picus_execute` is more than one round
  trip (a `prepare`, then the statement); the server's cancel key only interrupts what is
  running at the instant it arrives, so a Cancel landing in a gap hit nothing and was lost.
  `PgSession` pairs a run ordinal with a cancelled ordinal — scoped to an ordinal so a
  cancel arriving after a query finished cannot kill the next one.
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
| `picus-emit` (`block.rs` / `statement.rs` / `literal.rs`) | every dialect difference in emission, as `match` arms |
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

### The plan of 2026-07-27 (evening), in waves

Three cantieri that touch disjoint files run together, then two dependent waves.

**Wave A, parallel.**
- ~~**A1 — the generator on the real emitter.**~~ **Done.** `stores/picus/dml.svelte.ts` calls
  `picus_emit` / `picus_validate_rows` from a debounced effect and caches the result per target,
  so `sqlFor()` stays a pure read — the preview has no refresh button, and materialising the
  cache inside a `$derived` is the `state_unsafe_mutation` trap `queryStore` already paid for.
  `canGenerate` additionally requires the current model to have *been checked*: with the check
  on the far side of an IPC call, "nothing reported" is also what an unexamined model looks
  like. `ruleConflict` — which the stand-in had no concept of — is surfaced on the target's row
  and above the preview. `parseCsv` / `proposeCsvMapping` moved to `utils/picus/csv.ts` and
  `parsePastedInserts` parked in `utils/picus/paste-sql.ts` until A2 serves it;
  `mock-emit.ts` is deleted.
- **A2 — `picus-parse`.** The grammar (see the reasoning in §6).
- **A3 — encoding in `arbor-fs`.** The detection chain plus the folder-inheritance context.

**Wave B, on top of the parse.** `picus-project` (discovery + `.arbor/picus/project.toml`),
`picus-inventory` (object → file → coverage), `picus-analyze` (the twelve real rules plus
declared suppressions).

**Wave C, the part that writes.** `picus-rewrite` — **byte splicing over the original bytes,
never re-printing the file**, which is what makes the byte-identical round trip a theorem rather
than a hope — with transactional apply and rollback; then the script half of the frontend on
real RPC (`mock.ts` deleted); then docs, changelog and a keyboard pass.

**The seam, done (2026-07-28).** `picus-be` links parse / inventory / analyze / rewrite and
serves the six methods above. What was decided while wiring it:

- **Read once, invalidate by hand.** `PicusState` holds one snapshot per repository — every
  script decoded once, each entry carrying the SHA-256 of its bytes — and nothing expires on
  its own: a refresh or a write, and nothing else. A report that changes while nobody changed
  anything is a report people stop believing.
- **The parse is not cached**, because `ParsedFile` borrows its source and caching one beside
  its own source is self-referential. It is produced by one isolated function, which is also
  the only thing a future on-disk tier (content-addressed by that same digest) would replace.
  Measured on 400 files / 1.4 MiB: read+decode 117 ms, parse 848 ms, inventory 41 ms,
  analyse 170 ms — so the parse is two thirds of the cost and the obvious first lever, either
  a disk tier or `std::thread::scope` over the file list. The parallel parse is now in
  (`parse_all`, a parser per thread, capped under the core count so a scan does not make the
  window it is filling stutter). What that measurement did **not** show is below.
- **The real cost was line numbers, and it was quadratic.** A repository of ~500 files / 11 MB
  took over five minutes to index, and the profile put twenty-five of twenty-nine seconds in
  `line_col` — which counts newlines from byte zero, and was being called once per inventory site,
  per finding and per suppression. Linear per call, asked once per interesting position, is
  **O(bytes²) per file**: fine on 400 small scripts, catastrophic on a few large ones, which is
  why the 1.4 MiB measurement above missed it entirely. `ParsedFile` now carries a `line_starts`
  index built once per parse, and every caller holding a parse binary-searches it
  (`line_of` / `line_col_at`); `line_col` survives only for callers that have a source and no
  parse. Measured on an 11 MB fixture, the penalty for holding the bytes in a few large files
  rather than many small ones went from **15× to 0.7×** in `Inventory::build` and from **13.5× to
  0.8×** in `analyze`. `Context::lane` was the same shape of mistake one layer up — a fresh walk
  of the whole tree on each of ~8000 calls to answer ten distinct questions — and now resolves
  every lane once. There is a timing harness at `crates/products/picus/be/tests/perf.rs`, marked
  `#[ignore]`: `cargo test --release -p picus-be --test perf -- --ignored --nocapture`.
- **A write is two calls with a staleness check.** The preview returns the exact bytes and a
  digest per file; the apply re-prepares and refuses if any digest moved, naming the file. What
  was approved is what gets written, or nothing is.
- **The insertion point is a stated rule per role**, resolved repository (`[generation.insertion]`
  in `project.toml`) → user (`config.toml`) → built-in (update appends, everything else groups
  by table), and written into the diff's hunk header. A block Picus already wrote is found by
  its marker and **replaced**; its extent stops at the first statement about another table, so a
  hand-written statement below a generated block is never swallowed.
- **A script repository belongs to a connection** — `ConnectionSpec.scriptRoot`, persisted in
  `connections.toml`. Those scripts install *that* database.

One thing known in advance: `picus_emit` currently takes `model` and `targets` and knows nothing
about the marker template or the naming scheme, so its signature grows in Wave C. Better to
expect it than to watch it change twice.


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
  `DEFAULT_QUERY_TEXT`.
- ~~The generator on `picus_emit` / `picus_validate_rows`.~~ **Done** — see A1 above.
- Replace the remaining `ipc/picus/mock.ts` (project tree, inventory, findings) with real RPC as
  the script crates land, and `utils/picus/paste-sql.ts` with `picus_parse_inserts`.
- ~~Settings persist to `…/picus/config.toml` via `get_picus_config` / `set_picus_config`.~~
  **Done** for the product settings (never `localStorage`).
- Project-level settings (encoding, EOL, version table) are still in memory: they belong in the
  project's own config so a colleague opening the same repository inherits them. They persist
  when the script half gives the backend a project to attach them to.

### Editor intelligence (§4.4)

1. ~~**Ghost text in the shared core.**~~ **Done (2026-07-27):**
   `shared/ui/code-editor/inline-completion.ts` — the `intel.inlineCompletion` hook, a widget
   decoration, and the Tab/Esc keymap, installed by `createCodeEditorExtensions`. Bennu can
   adopt it by filling in one field. Three decisions worth knowing before using it:
   - the source is a **facet**, not module state, because two editors are alive at once in a
     tabbed window and module state would give whichever mounted last to both;
   - ghost text **never appears while the completion popup is open** — both want Tab, and the
     popup is the more specific intent, so ghost text stands down rather than competing;
   - a reply is dropped unless the caret is still exactly where the question was asked. A
     stale suggestion arriving late is worse than none.
   Still to write for Picus: the source itself (the deterministic rules below).
2. ~~**A SQL completion source**~~ ~~**A SQL hover source**~~ ~~**Live diagnostics**~~
   ~~**Deterministic ghost-text rules**~~ **All four done (2026-07-27):**
   `components/picus/sql-intel/` — a scanner (`tokens.ts`), one-statement analysis with alias
   resolution (`analysis.ts`), per-dialect vocabularies (`keywords.ts`), the catalogue gate
   (`schema-view.ts`), and the four sources. `picus-sql-language.ts` stays a descriptor and
   caches one per `(dialect, connection)` pair; the views pass the tab's connection, so both
   the dialect and the catalogue come from the tab and never from a global. Decisions worth
   knowing:
   - **`SchemaView.known` is the single gate** between "does not exist" and "not read yet". It
     is false unless the snapshot in the store describes *this* connection, is not loading,
     carries no error and is not empty — and then no object diagnostic is emitted at all.
   - **DDL is never measured against the live schema**, and anything the buffer creates
     earlier counts as existing. Otherwise an initialisation script — the kind of file Picus
     exists for — would be one long list of "unknown table".
   - **Unqualified names are never reported as unknown**, only as ambiguous. A bare word can
     be an output alias, a function or a PL/SQL variable; the scanner cannot tell.
   - A **script file borrows the active connection's catalogue only when the dialects agree**;
     with no match it completes keywords and closes blocks and says nothing about objects.
   - Ghost text implements the four rules from §4.4 and stops there. Rejected as guesses: the
     `SELECT` column list, a PK predicate after `DELETE FROM t`, every column after
     `UPDATE t SET`, and `$$ LANGUAGE plpgsql;` (a `DO $$` block's `$$;` *is* offered).
   - The hover card classes moved from `.bennu-hover` / `.bh-*` to `.cm-hover-card` /
     `.cm-hc-*` in the shared theme, so Picus composes the same card instead of forking it.
   Still open: the editors have **no Find panel** — `Ctrl+F` is host-routed (`openSearch()`
   via `bind:this`) and Picus never wires it, unlike Bennu.

### Known frontend debt

- `PicusToolbar.svelte` branches by tab kind and is getting long — split into one toolbar per
  document type before it becomes the place where "which button applies to what" hides.
- `CreateWorkspaceModal` / `GroupFormModal` (Corvus) can now use `ColorPalettePicker`, and
  `TytoShortcutsModal` can use `ShortcutsReference`. Both are a few lines; not done to avoid
  touching working products unasked.
- `shared/ui/Tree.svelte` still uses native HTML5 drag-and-drop, which WebView2 drops.

### Tests

None of these needs a live database. The count is deliberately not written down — it was
"29" here for long enough to become wrong by a factor of two — so ask the workspace instead:

```bash
cargo test -p picus-core -p picus-db-api -p picus-db-postgres
```

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

1. ~~**`NamingScheme`** for update scripts.~~ **Decided (2026-07-27):** a **default scheme plus a
   configurable regex**. The default is versioned — `4_12__4_13.sql`, the "from" read as the
   highest version present in the update folder and the "to" proposed by incrementing the last
   segment, both editable. A project whose files are named otherwise declares its own pattern in
   `.arbor/picus/project.toml` (named capture groups `from` / `to`, plus a template for new
   files), so no repository is locked out by a convention it never adopted. When a connection is
   open, the database's own version is shown next to the proposal and a mismatch is *reported*,
   never enforced. `VER003` (unbroken version chain) is only meaningful when the pattern yields
   both bounds, so it is skipped — with a visible reason — when it does not.
2. ~~**Crate granularity.**~~ **Decided (2026-07-27):** create each crate as it is activated,
   the way Bennu and Merula grew. No empty scaffolds.
3. ~~**Encoding detection placement.**~~ **Decided (2026-07-27):** all of it in **`arbor-fs`**,
   including the folder-inheritance rule for ASCII-ambiguous files. The user chose the shared
   home over a Picus-local layer. The containment condition: the additions are **additive** and
   existing callers (Bennu, Corvus) keep byte-identical behaviour unless they opt in — the
   folder context is an explicit object the caller builds, not a new implicit step in the
   existing read path.
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

### Decisions taken on 2026-07-28

| Question | Decision |
|---|---|
| The repository's shape | the **real directory tree**; dialect and role are declared on any folder and **inherited** by descendants. `Branch` deleted |
| What the rules compare | a **lane** = (effective dialect, effective role). Coverage is summed across a lane's folders, so `2024/ORA` and `2025/ORA` never report each other as gaps |
| Inventory columns | folded to **engine × role** for display, never one per folder — a repository has hundreds, eleven of them called `ORA`. Expanding a row gives the per-folder detail |
| Dialect inference | **whole-word** matching, not substring: every folder in the tree is now asked, and substring `ora` would have declared Oracle folders in the middle of other people's repositories (`LAVORAZIONE`, `ORARI`) |
| `POS` / `MSQ` as **built-in** keywords | **no** — too generic to guess safely. They would misclassify `POSIZIONI` in somebody else's repository, which is exactly what the whole-word tightening was for |
| …so how does a real repository say it | a **per-project vocabulary** (`[[alias]]` in `project.toml`), keyed by folder **name**. One line covers all eleven `POS` folders *and every one added later*, which folder-by-folder declaration can never do |
| Alias precedence | `[[folder]]` **>** `[[alias]]` **>** built-in keywords. A specific answer beats a general rule; a local fact beats a global heuristic |
| Engines Picus cannot read (`MSQ` = SQL Server, `DB` = DB2) | a **third state**, `ForeignEngine` — recognised, not supported. Named on screen, never asked about, and **never parsed**: a permissive grammar turns T-SQL into plausible nonsense. Standard names (`sqlserver`, `mssql`, `db2`, …) are built in; abbreviations stay per-project |
| Folders of portable SQL, valid on both engines | a **fourth state**, `Generic`. **Never inferred** — only declared, per path or by name. It **counts for every dialect** (in both lanes, satisfies `CONS001` on both sides), `DIA001` **inverts** there (a construct belonging to *either* engine is a broken promise), and generation is **allowed but restricted to the intersection** |
| Generation into a portable folder | **allowed**, not refused — the payoff is one file where two were needed. `Target.dialect` became a `DialectScope`, so there is no `EngineKind` to default to and every dialect-dependent decision in the emitter grew a portable answer or an `Err`. Refused: procedural block, version guard, upsert. Portable "now" is `CURRENT_TIMESTAMP`; identifiers are never folded |
| The portable coverage column | its **own** column in the inventory matrix rather than being counted into both dialects', so one INSERT never reads as two. `coverageGaps` knows a covered portable column at the same role means the dialect columns are not gaps — it has to agree with `CONS001`, which is read beside it |
| Offering the alias | a **second, distinct** confirmation right after a folder is classified, naming how many folders it would reach. Never a silent side effect of the first action, and declining costs the user nothing they just did |
| Where a repository is opened from | a **connection** owns it (`ConnectionSpec.scriptRoot`): Picus is database-oriented, so those scripts install *that* database |
| A repository whose engine is in the **file name** (`4_12_ORA.sql` beside `4_12_POS.sql`) | the engine becomes a property of the **file**, of which the folder is the default. `[[file]]` declarations, `ScriptFile.engine` / `effectiveEngine`, one inheritance rule extended by one link. The folder still owns the **role** |
| Whether the built-in vocabulary may read an engine out of a file name | **no, never.** `ORA` is Italian for *now* and `MIGRAZIONE_DA_MYSQL.sql` is a PostgreSQL script about MySQL; there are hundreds of file names to a dozen folder names and nobody reviews them. Only the project's own declarations classify a file |
| …so how does an alias reach file names | `applies_to = "folders" \| "files" \| "both"`, **opt-in**, absent meaning folders — what every alias already written means. It moves the engine only; a role stays a fact about a directory |
| A folder holding more than one engine, in the inventory | its coverage column **splits per engine** (`AGG · Oracle`, `AGG · PostgreSQL`, `AGG · unclassified`). One column would add the two dialects together and destroy the only comparison the table exists to make. Tidy folders keep byte-identical column names |
| What `CONS001` counts a lane's coverage from | the **sites**, not the folder-keyed coverage map. In a mixed folder that map is the folder's, so summing it per lane would credit one dialect with the other's statements — a false negative, the one wrong answer this rule must not give |
| What version number a project file is stamped with | the **lowest that can read it correctly** (`3` when something classifies an individual file, `2` otherwise), never the build's. The file is committed and shared: too new locks a colleague out, too old lets their build silently ignore a classification |

### Decisions taken on 2026-07-27

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
| What a "branch" is | **folders in one checkout**, not git branches — both dialects must be readable at once for the comparison to be instant. *Superseded 2026-07-28: there is no "branch" level at all; see §1* |
| Project config location | `<root>/.arbor/picus/project.toml`, proposed and written only on confirmation |
| One grammar or two | **one**, permissive superset — see below |
| Parse depth | **expression level**, including inside procedural blocks |
| Update-file naming | default versioned scheme + a per-project regex |
| Generated-block marker | configurable template with placeholders, `-- picus:` namespace |
| Encoding detection | entirely in `arbor-fs`, additive so Bennu/Corvus are unaffected |
| Scope of the 2026-07-27 evening wave | parse + project + inventory + analyze **and** rewrite/apply |

#### Why one grammar and not two

The reasoning, because it will look like a shortcut later and it is not: the two dialects
diverge almost always **by addition** (`MERGE … FROM DUAL` and `ON CONFLICT`, `CONNECT BY` and
`WITH RECURSIVE`, `q'[…]'` and `$$…$$`) and almost never **by collision** — the cases where the
same syntax means different things are few and all resolvable in the external scanner. At
expression level, two grammars would mean duplicating ~90 % of the rules (every operator, every
type, every expression) and fixing every bug twice.

But the deciding argument is diagnostic quality. With two strict grammars, an Oracle-ism inside
a PostgreSQL file is a parser `ERROR`, and the best message available is *"syntax error at line
12"*. With one permissive grammar it is **a node with a name**, and the message becomes
*"`MERGE … FROM DUAL` is Oracle syntax; PostgreSQL wants `INSERT … ON CONFLICT`"*. Diagnosing
cross-dialect drift is the product's entire reason to exist, so the grammar has to make that
drift **nameable** rather than make it explode. Consequence for the grammar's design: every
single-dialect construct gets its **own named node**, never folded into a generic one.
