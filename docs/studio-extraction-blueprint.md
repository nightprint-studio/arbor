# Studio → `crates/wasm/studio/*` — Code-Level Extraction Blueprint

> **Status: EXECUTABLE BLUEPRINT.** Supersedes `docs/studio-extraction-plan.md`
> (which was the high-level rationale). This document is the stage-by-stage
> instruction set an implementer follows. Every stage ends with `cargo check`
> green and adds unit tests. The human does the manual UI / round-trip pass per
> format; the agent's safety net is the per-crate test suite described in §6.

Naming follows the reorg rule: crates under `crates/wasm/` are `arbor-*`.
The `arbor-brp` crate (`crates/wasm/brp/`) is the template for crate shape
(`lib.rs` + `prelude.rs`, `#[cfg(test)] mod tests`, dep table).

---

## 1. Target crate layout

```
crates/wasm/studio/
├── types/        arbor-studio-types        — DTOs only, zero logic
├── core/         arbor-studio-core         — trait + generic format-agnostic logic
├── ron/          arbor-studio-ron          — RON AST + registry + backend + .rs schema
├── json/         arbor-studio-json         — JSON/JSONC AST + byte-splice + JSON-Schema
├── toml/         arbor-studio-toml         — TOML (toml_edit)
├── yaml/         arbor-studio-yaml         — YAML (yaml-edit + serde_yaml_ng)
├── properties/   arbor-studio-properties   — .properties (line-oriented)
└── api/          arbor-studio-api          — registry + scanner + dispatch facade (Tauri-free)
```

Workspace members are appended to root `Cargo.toml` (the explicit `members = [...]`
list) one crate at a time, as each gains a `src/lib.rs` — same discipline the
existing reorg uses.

### Dependency DAG (must stay acyclic)

```
types  ──────────────────────────────────────────┐
  ▲                                               │
core ──► types                                    │
  ▲                                               │
ron, json, toml, yaml, properties ──► core, types │
  ▲                                               │
api ──► core, types, {all 5 format crates}        │
  ▲                                               │
src-tauri (launcher glue) ──► api (+ types for IPC serde)
```

`core` and `types` depend on **no format crate** and **no Tauri**. Format crates
never depend on each other (today's `toml_studio`/`yaml_studio` reaching into
`json_studio::schema` and `ron_studio::schema` is a violation that this
extraction must break — see §3.5 "Schema provider seam").

### Per-crate ownership

| Crate | Owns | Key deps |
|---|---|---|
| **arbor-studio-types** | Every DTO crossing the trait/IPC boundary: `ParseResult`, `EncodingInfo`, `UpdateResult`, `MutateResult`, `NodeView`, `QueryHit`, `StudioMutation`, `DiffHunk`/`DiffLine`/`DiffLineKind`, `DiffTreeNode`/`DiffStatus`, `DocSnapshot`, `FileEntry`, `SchemaHint`/`SchemaHintOrigin`, `FormatDescriptor` + its sub-enums (`IconRef`, `NullPolicy`, `QuerySyntax`, `CrossRefScope`, `SchemaSourceKind`, `KindStyle`/`KindTone`, `SaveWarningKind`), the F12 set (`RenameSite`/`RenameSiteScope`/`RenameDirtyBlocker`/`RenameCollision`/`RenameOpenDoc`/`RenamePreview`/`RenameResult`/`RenameFailure`), the F13 set (`BulkEditAction`/`BulkEditLiteral`/`BulkEditValueSource`/`BulkEditScope`/`BulkEditSite`/`BulkEditOpenDoc`/`BulkEditPreview`/`BulkEditResult`/`BulkEditFailure`), and the schema-view DTOs (`CrateProbe`, `Schema`, `TypeSource`, `ResolvedType`, `TypeDef`, `FieldDef`, `VariantDef`, `VariantShape`, `RootCandidate`, `CandidateKind`, `SchemaStats`). The `StudioError`/`StudioResult` type also lives here. **No `impl` beyond derives + trivial ctors.** | `serde`, `serde_json`, `thiserror` |
| **arbor-studio-core** | The `StudioFormatBackend` trait. The generic engines that are copy-pasted ×5 today: `History<T>` (coalescing/cursor/cap), `diff::{unified, tree}` over `serde_json::Value`, `query::run` (JSONPath), `refactor::{rename_sites, bulk_sites, …}` (F12/F13 site enumeration + apply orchestration via small per-format trait callbacks), `edit_expr` (the F13 mini-language, moved verbatim), `persist` (encoding-aware read/write), and `DefaultBackend<T>` — the generic backend wrapper for "simple" formats (TOML/YAML/.properties). | `arbor-studio-types`, `serde_json`, `serde_json_path`, `similar`, `arbor-fs`, `async-trait`, `uuid` |
| **arbor-studio-ron** | `RonAst` + recursive-descent parser + pretty-printer + `to_json`/`json_to_ron`; the `.rs` schema loader (syn-based); `RonStudioRegistry`; hand-written `RonBackend` (RON is too special for `DefaultBackend<T>`: variant-tag preservation + inline `.rs` schema). | `core`, `types`, `syn`, `prettyplease`, `quote`, `serde_json_path` |
| **arbor-studio-json** | `JsonAst` (byte-range/Span) + jsonc-parser edit path + simd-json read path + byte-splice `edits` + JSON-Schema loader (`schema.rs`); `JsonStudioRegistry`; hand-written `JsonBackend` (dual parser, stream mode, JSONC). | `core`, `types`, `jsonc-parser`, `simd-json`, `serde_json_path` |
| **arbor-studio-toml** | toml_edit `DocumentMut` model, cursor-walk mutations, TOML `DefaultBackend<T>` impl of the `SimpleFormat` callbacks (see §2.6). | `core`, `types`, `toml_edit` |
| **arbor-studio-yaml** | yaml-edit `Document` + serde_yaml_ng projection, multi-doc stream split, YAML `SimpleFormat` impl. | `core`, `types`, `yaml-edit`, `serde_yaml_ng` |
| **arbor-studio-properties** | Line-oriented `RawLine` model, continuation/escape/unicode handling, projection w/ `$value` sentinel, .properties `SimpleFormat` impl. | `core`, `types` |
| **arbor-studio-api** | `StudioRegistry` (the `HashMap<format_id, Arc<dyn StudioFormatBackend>>`), `studio_registry()` factory that registers all 5 backends, the format-agnostic **cross-ref scanner** (`scan_repo`, `scan_cross_refs_for`, `find_usages_for`, `scan_broken_refs_for`) with its per-format `collect_defs_into` delegation, the `index` module, and the `dispatch(method, params)` entrypoint. Tauri-free. | `core`, `types`, all 5 format crates, `arbor-fs`, `serde_json` |

Each format crate exposes (via its `prelude`):
`pub fn backend() -> std::sync::Arc<dyn arbor_studio_core::prelude::StudioFormatBackend>`
and `pub fn collect_defs_into(ast: &serde_json::Value, … , out: &mut Vec<CrossRefDef>)`
for the scanner. The per-format `parse_to_value(text) -> Result<serde_json::Value>`
also moves with its crate (the scanner currently calls each format's projector).

Every crate gets `pub mod prelude;` re-exporting its public surface (workspace
rule). `types` and `core` also keep submodules `pub` for rustdoc navigation, but
call sites import through the prelude.

---

## 2. The generic `studio-core` API (concrete shapes)

All signatures below are the **target** shapes. They are lifted from the 5
copy-pasted implementations (RON `mod.rs` 92-126/759-800/1011-1057/281-421,
JSON `mod.rs` 121-131/1063-1340/1343-1356, and the TOML/YAML/.properties twins
the survey flags as "IDENTICAL pattern").

### 2.1 `core::history` — `History<T>`

Generic over the snapshot type (every format snapshots `String`, but keeping it
generic lets a future format snapshot a richer value).

```rust
pub struct History<T> {
    stack:   Vec<T>,
    pos:     usize,          // cursor: stack[pos] is the live state
    armed:   bool,           // coalesce gate (set by record_text, cleared by record_struct)
    last_at: Option<std::time::Instant>,
    cap:     usize,          // 200 (JSON/TOML/YAML/.properties) — RON uses 128; pass at ctor
    window:  std::time::Duration, // 500ms
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(initial: T, cap: usize) -> Self;             // window defaults to 500ms
    pub fn with_window(initial: T, cap: usize, window: Duration) -> Self;

    /// Text-level edit (textarea typing). Coalesces into the previous entry
    /// when armed AND within `window`; otherwise pushes a new entry. Always
    /// drops the redo tail (everything after `pos`). Re-arms.
    pub fn record_text(&mut self, snap: T);

    /// Structured edit (tree mutation). NEVER coalesces. Pushes, drops redo
    /// tail, drains oldest on overflow, dis-arms (so the next text edit can't
    /// fold into a structural entry).
    pub fn record_struct(&mut self, snap: T);

    pub fn undo(&mut self) -> Option<&T>;   // pos -= 1 if pos > 0
    pub fn redo(&mut self) -> Option<&T>;   // pos += 1 if pos < len-1
    pub fn current(&self) -> &T;
    pub fn can_undo(&self) -> bool;          // pos > 0
    pub fn can_redo(&self) -> bool;          // pos < len-1
}
```

> **Invariant the test suite locks:** coalescing collapses N rapid `record_text`
> within the window into one undo step; a `record_struct` between two
> `record_text` always breaks the chain (you get 3 undo steps, not 1). Overflow
> drains from the front and keeps `pos` valid. This is the #1 behavior-sensitive
> generic — see §6 history tests.

### 2.2 `core::diff` — unified + tree diff over `serde_json::Value`

```rust
/// Line-level unified diff (similar crate, 3-line context, grouped hunks).
pub fn unified(original: &str, current: &str) -> Vec<DiffHunk>;

/// Recursive structural diff. Walks two Values, status =
/// Added/Removed/Modified/Partial/Unchanged with child-change rollup;
/// Unchanged subtrees are pruned (only Partial containers keep children).
/// `change_count` is the leaf-change tally.
pub fn tree(before: &serde_json::Value, after: &serde_json::Value) -> DiffTreeNode;
```

Both `DiffHunk`/`DiffLine` and `DiffTreeNode`/`DiffStatus` are `types`. RON's
tree-diff has format-specific shape-matching (struct/tuple name match required;
synthetic `Some` segment for Option) — that lives in **arbor-studio-ron** by
projecting the AST to the `$type`/`$tag`/`Some`-wrapped `Value` *before* calling
`diff::tree` (the projection already exists as `project_for_query`). The generic
`tree` only ever sees `serde_json::Value`.

### 2.3 `core::query` — JSONPath

```rust
/// Normalise shorthands (`foo` → `$..foo`, `.foo` → `$.foo`, leading-$ insert),
/// run serde_json_path against `root`, dedup by path, cap at `max_hits` (500).
/// Returns (path_segments, located_value) pairs; the caller maps to QueryHit
/// with its own kind/preview/variant_tag.
pub fn run(root: &serde_json::Value, expr: &str, max_hits: usize)
    -> Result<Vec<QueryLoc>, QueryError>;

pub struct QueryLoc { pub path: Vec<String>, pub value: serde_json::Value }

pub fn normalise(expr: &str) -> String;   // exposed for reuse + direct testing
```

Format-specific bits stay in the format crate: building the projected `Value`
(`project_for_query`) and resolving the JSONPath path back to a live-AST path
(RON strips synthetic `$items` segments; .properties flattens dotted keys).

### 2.4 `core::edit_expr` — F13 mini-language

Move `src-tauri/src/studio/edit_expr.rs` **verbatim** into `core` (it's already
self-contained: `Value`, `compile`, `CompiledExpr::eval`). No format coupling.
This is a pure parser/evaluator — high test value, zero migration risk.

### 2.5 `core::refactor` — F12/F13 site enumeration + apply orchestration

The survey shows F12/F13 is ~400 LOC/format of which ~300 is identical
orchestration (file grouping, dirty-blocker check, collision detect, atomic
pre-flush, encoding round-trip). Extract the orchestration; inject the
per-format leaf operations via a small trait.

```rust
/// Per-format hooks the refactor orchestrator needs. Implemented once per
/// format crate (RON/JSON hand-written, simple formats via DefaultBackend).
pub trait RefactorOps {
    /// Project a parsed doc text to the Value used for query/site matching.
    fn parse_to_value(&self, text: &str) -> Result<serde_json::Value, StudioError>;
    /// Apply a string rename to every matching path; re-emit text (lossless
    /// where the format allows). Used by F12 apply.
    fn apply_string_rename(&self, text: &str, old: &str, new: &str)
        -> Result<String, StudioError>;
    /// Apply a batch of (path, op) bulk edits; re-emit text. 2-phase
    /// (sets then deletes in reverse index order) is the orchestrator's job;
    /// this just splices one resolved op.
    fn apply_bulk_ops(&self, text: &str, ops: &[BulkOp])
        -> Result<String, StudioError>;
    /// Coerce an edit_expr::Value / literal to the format's typed set-value,
    /// honoring the target node kind (Int vs Float, Option-wrap, null policy).
    fn coerce_set_value(&self, target_kind: &str, raw: &edit_expr::Value)
        -> Result<BulkOp, CoerceSkip>;
}

/// F12 — build the preview from the project index + open-doc state.
pub fn rename_preview(
    idx: &StudioIndex, kinds: &[StudioFileKind],
    old_value: &str, new_value_hint: Option<&str>,
    open_docs: &[RenameOpenDoc],
) -> RenamePreview;   // sites + dirty_blockers + collisions

/// F12 — apply (atomic pre-flush): parse+rewrite all in memory, then flush.
pub async fn rename_apply<O: RefactorOps>(
    ops: &O, repo_root: &str, old: &str, new: &str,
    sites: Vec<RenameSite>, open_docs: Vec<RenameOpenDoc>,
) -> RenameResult;

/// F13 — preview (active-doc OR project-wide), expression compiled once.
pub async fn bulk_preview<O: RefactorOps>(/* … */) -> BulkEditPreview;
pub async fn bulk_apply<O: RefactorOps>(/* … */)  -> BulkEditResult;
```

The dirty-blocker / collision / `synth_preview_line` / `canonicalise_path_key`
helpers (identical across all five `backend_impl.rs`) also move here as private
functions of `core::refactor`.

### 2.6 `core::DefaultBackend<T>` — the simple-format wrapper

This is the lever that deletes ~450 LOC of type-mapping boilerplate ×3
(TOML/YAML/.properties). It implements the full `StudioFormatBackend` trait once
against a per-format `SimpleFormat` trait that exposes only the format-specific
primitives.

```rust
pub trait SimpleFormat: Send + Sync + 'static {
    fn descriptor(&self) -> &FormatDescriptor;

    // doc lifecycle on owned text + projected Value
    fn parse(&self, text: &str, encoding: &EncodingInfo) -> ParseOutcome;
    fn project(&self, doc: &Self::Doc) -> serde_json::Value;        // for query/tree/children
    fn emit(&self, doc: &Self::Doc) -> String;                      // current text
    fn detect_indent(&self, text: &str) -> String;

    // structured mutations (each returns the new text or an error)
    fn mutate_primitive(&self, doc: &mut Self::Doc, path: &[String], v: PrimitiveValue) -> R;
    fn insert_field(&self, …) -> R;  fn insert_item(&self, …) -> R;
    fn insert_map_entry(&self, …) -> R; fn remove_at(&self, …) -> R;
    fn duplicate_at(&self, …) -> R;  fn move_item(&self, …) -> R;
    fn replace_at(&self, …) -> R;

    // node metadata for NodeView/QueryHit
    fn node_kind(&self, v: &serde_json::Value) -> String;
    fn preview_for(&self, v: &serde_json::Value) -> String;
    fn variant_tag(&self, v: &serde_json::Value) -> Option<String>;  // None for non-RON

    type Doc;
}

/// Owns: doc registry (HashMap<doc_id, DocState<F::Doc>>), History<String>,
/// encoding, parse_error, indent, original/current snapshots — ALL the
/// boilerplate. Implements StudioFormatBackend + RefactorOps by calling F's
/// primitives + core::{history, diff, query, refactor}.
pub struct DefaultBackend<F: SimpleFormat> { /* Mutex<HashMap<…>>, F */ }
```

RON and JSON do **not** use `DefaultBackend<T>` (variant tags + dual-parser/byte-
splice + .rs/.json schema make them too special); they implement
`StudioFormatBackend` + `RefactorOps` by hand but **still call** `core::history`,
`core::diff`, `core::query`, `core::edit_expr`, and the `core::refactor`
orchestrators. The win there is the engines, not the wrapper.

---

## 3. Trait + shared types: what moves, how formats re-implement

### 3.1 The trait

`StudioFormatBackend` (today `src-tauri/src/studio/format/backend.rs`) moves to
`core` **unchanged** in signature. The private `StudioFormatBackendIdHelper` /
`descriptor_id()` match moves with it. `async-trait` dep comes along.

### 3.2 Errors

`StudioError`/`StudioResult` move to `types`. **One change required:** today
`StudioError::App(#[from] AppError)` couples to the launcher's `crate::error`.
In `types`, replace with `StudioError::App(String)` (or a thin
`StudioError::Backend { message: String }`). The launcher's `to_ipc` already
stringifies, so the IPC surface is identical. Each format crate maps its own
parse/IO failures to `StudioError::Backend`. **This breaks the last launcher
coupling for the format crates.**

### 3.3 DTOs

All DTOs in `studio/format/types.rs` + the schema-view types currently
re-exported from `ron_studio::schema` move to `types`. The schema-view types
(`CrateProbe`, `Schema`, `TypeSource`, `ResolvedType`, `TypeDef`, `FieldDef`,
`VariantDef`, `VariantShape`, `RootCandidate`, `CandidateKind`, `SchemaStats`)
become **plain DTOs in `types`** (they are the wire shape the FE schema panel
consumes). The RON `.rs` schema *loader logic* stays in `arbor-studio-ron`; it
just produces these `types` DTOs now instead of owning them.

### 3.4 Descriptor

`FormatDescriptor` + all sub-enums move to `types`. Each format crate keeps its
`build_descriptor()` factory (the hard-coded capability matrix) in its own crate,
returning the `types::FormatDescriptor`.

### 3.5 Schema provider seam (breaks the format↔format cycle)

**Today TOML/YAML/.properties `backend_impl.rs` call
`crate::json_studio::schema::{probe,load,get_type_source}` and `crate::ron_studio::schema`
directly** — a format-to-format dependency the new DAG forbids. Fix: define in
`core` a `SchemaProvider` trait:

```rust
#[async_trait]
pub trait SchemaProvider: Send + Sync {
    async fn probe(&self, source: &str) -> StudioResult<CrateProbe>;
    async fn load(&self, source: &str, root_canonical: &str) -> StudioResult<Schema>;
    async fn view_source(&self, source: &str, canonical: &str) -> StudioResult<TypeSource>;
}
```

`arbor-studio-ron` exposes a `RsSchemaProvider` (the syn loader);
`arbor-studio-json` exposes a `JsonSchemaProvider`. The `arbor-studio-api`
registry **injects** the right provider(s) into each `DefaultBackend` at
construction (TOML gets both Rust+JSON, YAML/.properties get JSON-only).
The format crates no longer name each other.

---

## 4. Launcher glue that STAYS, and exactly what changes

The launcher keeps the **Tauri command/dispatch shell** — it is the host, not a
library, so it has no prelude and is not consumed by other crates.

**Stays in `src-tauri`:**
- `commands/rpc_commands.rs::rpc` — the single `#[tauri::command]` entrypoint
  (`spawn_blocking` → dispatch). Unchanged.
- `studio/mod.rs::dispatch` (lines 43-67) — but now forwards to
  `arbor_studio_api::dispatch` instead of holding the registry itself.
- `ipc/studio/*` handler modules (`index.rs`, `config.rs`, `format.rs`) — the
  `#[studio::handler(program="studio")]` self-registering handlers. They keep
  their IPC wiring but call into `arbor-studio-api` for the actual work.
- `app_state.rs` — `studio_registry: Arc<StudioRegistry>` field stays, but
  `StudioRegistry` is now `arbor_studio_api::StudioRegistry`; `AppState::new`
  calls `arbor_studio_api::studio_registry()` instead of the 5 inline
  `register(crate::*_studio::backend_impl::backend())` calls.
- `studio/config.rs` (sidecar `.arbor/studio.toml` + glob excludes) — stays in
  launcher **only if** it touches `config::corvus_read`; otherwise move to `api`.
  Audit in Stage 0.

**Changes in `src-tauri`:**
- **Deps removed** from `src-tauri/Cargo.toml` once the code leaves (Stage 3+):
  `simd-json`, `jsonc-parser`, `syn`, `prettyplease`, `quote`, `yaml-edit`,
  `serde_yaml_ng`, `toml_edit`. `serde_json_path` and `similar` move to `core`
  but `similar` *also* stays in the launcher (in-shell text diff — see the
  comment at `src-tauri/Cargo.toml:103`). `serde_json`/`toml`/`serde` stay.
- **Deps added**: `arbor-studio-api` (path dep) + `arbor-studio-types` (the IPC
  layer (de)serializes the DTOs).
- The 5 `*_studio/` module dirs are deleted from `src-tauri/src/`; the
  `studio/format/*` (trait/types/descriptor/errors/registry) modules are deleted
  (now in `types`/`core`/`api`); `studio/mod.rs` scanner + `studio/index.rs`
  move to `api` (the launcher's `studio/mod.rs` shrinks to the dispatch shim).

**FE is unaffected** — the `studio(method, params)` IPC contract is byte-identical
through the whole backend extraction (Stages 1-4). Only Stage 5 touches FE.

---

## 5. Staged execution — compile gates + tests per stage

> Each stage: ends `cargo check` green for the whole workspace, adds the listed
> tests (`cargo test -p <crate>` green), is an individually revertible commit on
> the `feature/launcher` (or a dedicated `studio-extract`) branch. The human runs
> the manual round-trip pass after the stages that touch behavior (2, 3).

### Stage 0 — Coupling audit (no code move)
Grep every `crate::` symbol the `studio`/`*_studio` modules import from the
launcher. Confirm the only real couplings are: `crate::error::{AppError,Result}`
(→ becomes `StudioError::Backend(String)`), `arbor_fs::prelude::encoding`
(already a crate dep — fine), `config::corvus_read` (decide: stays in launcher
glue or moves to `api`). Produces the exact import-rewrite list. **No gate**
(audit only) — output is a checklist consumed by Stage 1.

### Stage 1 — `types` + `core` skeleton
1. `cargo new --lib crates/wasm/studio/types` + `core`; add to workspace members;
   add `prelude.rs`.
2. Move all DTOs + descriptor + `StudioError` into `types` (apply the
   `App(String)` change from §3.2).
3. Move the trait into `core`; create empty `core::{history, diff, query,
   edit_expr, refactor, persist}` modules (skeletons, not yet wired).
4. Rewire the 5 launcher backends to `impl arbor_studio_core::…::StudioFormatBackend`
   and import DTOs from `arbor_studio_types`. **Behavior unchanged** — the
   per-format logic is still inline in `src-tauri`, just pointing at the new
   trait/DTO crates.

**Gate:** workspace `cargo check`.
**Tests added:**
- `types`: serde round-trip (`serde_json::to_value` → `from_value`) for the 3
  enums most likely to drift on the move — `StudioMutation`, `BulkEditValueSource`,
  `DiffStatus`/`DiffTreeNode` — plus `FormatDescriptor` (the FE gates on its
  flags; a serde rename slip there silently disables an affordance).

### Stage 2 — Lift generics into `core`, one at a time
Do these as **separate commits**, each followed by a manual round-trip check.
Order = least → most behavior-sensitive is wrong; do most-mechanical first:

**2a — `edit_expr`** (move verbatim). All 5 backends `use arbor_studio_core::prelude::edit_expr`.
  - Tests: parser + evaluator (see §6 query/expr).
**2b — `diff::{unified, tree}`**. Rewire all 5 `unified_diff`/`tree_diff`/`build_tree_diff`
  to call `core::diff`. RON projects to Value first.
  - Tests: §6 diff.
**2c — `query::{run, normalise}`**. Rewire all 5 query methods.
  - Tests: §6 query.
**2d — `History<T>`**. Replace each format's hand-rolled `record_history`/
  coalesce/undo/redo with `core::history::History<String>` (cap 200; RON 128).
  - Tests: §6 history. **Highest-risk generic** — the coalescing semantics
    are subtle (the human must undo/redo across typing + tree edits per format).
**2e — `refactor` orchestrators + `RefactorOps`**. Each backend implements
  `RefactorOps` (4 small fns) and delegates F12/F13 preview+apply to
  `core::refactor`. Deletes the ~300 LOC ×5 orchestration.
  - Tests: §6 refactor.
  - **DONE.** `core::refactor` (site building, collision, dirty-blocker,
    bulk site/op building w/ skip taxonomy, atomic multi-file flush via
    `core::persist`) + `RefactorOps` (`parse_to_value` /
    `apply_string_rename` / `apply_bulk_ops` / `coerce_set_value` /
    `node_kind` / `preview_for`). The encoding-aware read/write flush
    lives in `core::persist` (uses `arbor-fs` encoding). 2-phase
    set-then-delete-reverse-index ordering stays in each leaf
    `apply_bulk_ops` (it owns the mutation engine), per the real code.
    **JSON / TOML / YAML** delegate F12+F13 to `core::refactor` (each via
    a unit `*Refactor` struct impl + a tiny `to_*_ops` lowering). Index
    aggregation → core inputs goes through `studio/refactor_glue.rs`
    (core must not name the launcher's `StudioIndex`).
    **RON stays special** (unfiltered index, indent-carrying
    `apply_string_rename`, `Result`-returning op builder, separate
    parse/apply/pretty, `raw_current` active-doc path — same precedent as
    RON's special diff/query). **`.properties` stays special** (F12: the
    rename leaf needs per-site Key/Value scope + old_value to rename the
    dotted *key* itself, not the `(paths, new)` seam; F13: every value
    coerces to a string with an `(empty)` sentinel + a divergent preview).
    Both still call `core::persist` indirectly is N/A — they keep their
    own flush; correctness > dedup. 14 new `core::refactor` unit tests
    (stub `RefactorOps` + synthetic index).

**Gate after each sub-stage:** `cargo check` + `cargo test -p arbor-studio-core`.
**Manual after 2d, 2e:** per-format undo/redo + F12 rename + F13 bulk on a sample
repo.

### Stage 3 — Per-format crates
For each format (do RON and JSON last — they're the special ones):
1. `cargo new --lib crates/wasm/studio/<fmt>`; add to members + prelude.
2. Move `<fmt>_studio/*` into it; move its format libs out of
   `src-tauri/Cargo.toml` into the crate's `Cargo.toml`.
3. TOML/YAML/.properties: refactor the backend onto `DefaultBackend<T>` +
   `SimpleFormat` (deletes the ~450 LOC type-mapping boilerplate). RON/JSON:
   keep hand-written backend, but it now lives in its own crate and calls `core`.
4. Wire the §3.5 `SchemaProvider` injection (TOML/YAML/.properties no longer name
   ron/json crates).
5. Expose `backend()`, `collect_defs_into`, `parse_to_value` via prelude.

**Gate per format:** `cargo check` + `cargo test -p arbor-studio-<fmt>`.
**Tests added:** §6 per-format round-trip (the crown-jewel tests).
**Manual per format:** open → edit → undo/redo → save, diff byte-identical
except intent (comments/quotes/variant-tags survive).

### Stage 4 — `api` facade + launcher glue
1. `cargo new --lib crates/wasm/studio/api`; add members + prelude.
2. Move `studio/mod.rs` scanner + `studio/index.rs` into `api`; move the registry
   (`studio/format/registry.rs`) into `api`; write `studio_registry()` +
   `dispatch(method, params)`.
3. Launcher: delete the now-empty `*_studio/` dirs + `studio/format/*`; shrink
   `studio/mod.rs` to the dispatch shim; `app_state.rs` calls
   `arbor_studio_api::studio_registry()`; remove the studio-only deps from
   `src-tauri/Cargo.toml`; add `arbor-studio-api` + `arbor-studio-types`.

**Gate:** workspace `cargo check`.
**Tests added:** `api` — scanner over a fixture repo dir (defs/usages/broken-refs
counts for a known fixture), registry `get`/`list_descriptors`, dispatch routes a
known method to the right backend.
**Manual:** smoke all 5 formats end-to-end (the FE still talks the same IPC).

### Stage 5 — Frontend collapse
1. Promote `StudioModal.svelte` into the generic `Studio.svelte` (owns Modal,
   view-mode tabs, right-rail, sidecar snippet routing). Instantiate the 3
   per-modal composables (`useStudioEditPipeline`, `useStudioRenameBulkPipeline`,
   `useStudioSchema`, `useStudioGlobalKeys`) **once** in `Studio.svelte`, taking
   format-specific lambdas as props.
2. Collapse the 5 per-format modals to thin wrappers (~50-100 LOC each) that
   pre-bind backend `format` + the format-specific lambdas (`computeSeed`,
   `commit` type-narrowing, `walkType` schema walker, `rowEditMode`,
   `isPromotableNull`, container builders) + the format-specific UI sections
   (JSONC banner, RON workspace tabs, YAML null toggles, TOML taxonomy,
   Properties↔YAML converter) via the snippet API.
3. The 6 already-shared composables stay format-agnostic.

**Gate:** `yarn svelte-check` (run by the human per hard-rule #1 — agent does the
edits, human compiles).
**Tests:** none added (Svelte); rely on the human's manual UI pass per format.
**Docs:** update `PluginDevelopment.svelte`/`GettingStarted.svelte` only if a
user-visible Studio affordance changed (it shouldn't — pure refactor).

---

## 6. Per-crate unit-test plan (pure logic)

The test suite is the agent's correctness net (no runtime). Test pure logic; skip
Tauri/FS-destructive glue (that's the human's manual pass).

### `arbor-studio-types`
- Serde round-trip for the discriminant-bearing enums (`StudioMutation`,
  `BulkEditAction`/`BulkEditValueSource`/`BulkEditScope`, `DiffStatus`,
  `RenameSiteScope`, `NullPolicy`/`QuerySyntax`). Catches a `#[serde(rename)]`
  slip that would silently mis-route a mutation or disable an FE affordance.
- `FormatDescriptor` serialize → assert the capability-flag field names the FE
  reads (`supports_rename_reference`, `supports_bulk_edit`, …) are present.

### `arbor-studio-core::history`
- `record_text` ×N within window = 1 undo step; outside window = N steps.
- `record_text`, `record_struct`, `record_text` = 3 steps (struct breaks coalesce).
- undo then `record_*` drops the redo tail.
- overflow past `cap` drains oldest, `current()`/`can_undo` stay correct.
- `can_undo`/`can_redo` at boundaries (fresh doc, fully-undone, fully-redone).

### `arbor-studio-core::diff`
- `unified`: identical input → no hunks; single line change → 1 hunk w/ correct
  old/new line numbers; multi-hunk with context grouping.
- `tree`: equal Values → Unchanged/pruned; added key → Added; removed → Removed;
  changed leaf → Modified; nested change → parent Partial with `change_count`
  rolled up; sibling union (removed-in-after) tracked.

### `arbor-studio-core::query`
- `normalise`: `foo`→`$..foo`, `.foo`→`$.foo`, `$.a.b` untouched, bracket form.
- `run`: dedup identical paths, cap at `max_hits`, returns correct path segments
  for `$..name`, empty result on no match, error surfaces on malformed expr.

### `arbor-studio-core::edit_expr`
- Parse + eval each precedence level (ternary, `??`, `||`/`&&`, comparisons,
  arithmetic, unary, postfix method calls, template strings).
- `old` binding; method-on-null short-circuits to the "skip site" error; `??`
  null-guard; strict typing (no implicit cross-kind coercion); `.to_*()` casts.

### `arbor-studio-core::refactor`
- `rename_preview`: site collection from a synthetic index, collision detection
  when `new_value_hint` already exists, dirty-blocker when an `open_doc` is dirty.
- `bulk_*`: site building with skip reasons (container hit, eval error,
  null-on-non-option), 2-phase op ordering (deletes reverse-index), applied vs
  skipped counts.
- Coercion via a stub `RefactorOps`: Int+integral→Int else Float, Option-wrap of
  null, type-mismatch → skip.

### Per-format crate (the crown jewels — round-trip preservation)
For **each** of ron/json/toml/yaml/properties, a `parse → edit → undo/redo →
serialize` round-trip asserting the **format-specific invariant** survives:
- **ron**: variant tags (`Some(...)`, named struct/tuple) preserved; float `.0`
  forced; `to_json`/`json_to_ron` round-trip for structure.
- **json**: `.jsonc` comments + trailing commas survive a scalar edit (byte-
  splice is lossless); stream-mode threshold selects the right parse path;
  `strip_features` removes comments only when invoked.
- **toml**: comments/whitespace/key-ordering survive (toml_edit decor);
  array-of-tables vs array vs inline-table kinds distinct (FROZEN F11); datetime
  literal preserved; null→delete (FROZEN F13).
- **yaml**: scalar `SetPrimitive` lossless via `set_path`; multi-doc `---` split
  round-trips; comments on untouched subtrees survive; null=Native.
- **properties**: continuation lines (trailing `\`), key/value escapes
  (`\=`/`\:`/`\n`/`\uXXXX`), `$value` sentinel for prefix collisions
  (`foo=v` + `foo.bar=w`), every-key-is-ref projection.
- Each also: undo after a tree mutation restores byte-identical original;
  encoding round-trip (windows-1252 / UTF-16 BOM fixture decode→encode identity).

### `arbor-studio-api`
- Scanner over a fixture repo dir: `scan_repo` finds the right kinds;
  `scan_cross_refs_for`/`find_usages_for`/`scan_broken_refs_for` return expected
  counts for a hand-built fixture (defs + refs + one broken ref).
- Registry: `get("ron")` Ok, `get("xml")` → `UnknownFormat`; `list_descriptors`
  sorted by id, length 5.
- `dispatch`: a known method+params routes to the expected backend (assert on a
  no-op like `list_descriptors`).

---

## 7. Risks and how the tests cover them

| Risk (silent = config-file corruption) | Coverage |
|---|---|
| **History coalescing drift** — wrong undo granularity after the `History<T>` lift (the single most behavior-sensitive generic). | §6 history tests lock coalesce-within-window, struct-breaks-chain, redo-tail-drop, overflow. Manual undo/redo pass after Stage 2d. |
| **Lossless round-trip regression** — comments/quotes/variant-tags/decor dropped after a format moves crate or onto `DefaultBackend<T>`. | §6 per-format round-trip tests assert the exact format invariant; human does open→edit→undo→save→diff per format at Stage 3. |
| **F12/F13 atomicity / dirty-blocker / collision regression** when orchestration is extracted to `core::refactor`. | §6 refactor tests cover blocker, collision, 2-phase ordering, skip-reasons via stub `RefactorOps`; manual multi-file rename+bulk on a sample repo after Stage 2e + 4. |
| **Serde rename slip on a moved DTO** silently mis-routes a mutation or disables an FE flag (FE gates on descriptor flags + enum tags). | §6 types serde round-trip + descriptor field-name assertions. |
| **Format↔format dependency cycle** (TOML/YAML/.properties calling json/ron schema) reintroduced or mis-wired. | §3.5 `SchemaProvider` seam + DAG; `cargo check` fails the workspace on a cycle; `api` injection test exercises the provider wiring. |
| **Scanner stops delegating to a format** after the move (cross-refs vanish for one format). | §6 `api` scanner fixture asserts per-kind counts incl. all 5 formats. |
| **Encoding loss** (windows-1252 / UTF-16 BOM) after `persist` moves to `core`. | §6 per-format encoding decode→encode identity fixture. |
| **Tree-diff shape mismatch for RON** (struct/tuple name-match, synthetic `Some`) if projected wrong before `diff::tree`. | RON round-trip + diff tests assert variant/Option diff shape via the projection. |

---

## 8. Net effect

- ~10k LOC of duplicated history/diff/query/refactor/type-mapping collapses into
  `arbor-studio-core` (one place to fix a bug, one place to test).
- 8 Tauri-free crates that are exactly the WASM-guest compilation target (this
  extraction = step 1 of Studio-as-WASM-plugin; see `docs/wasm-plugin-integration.md`).
- Launcher loses 8 heavy parser deps and ~14k LOC of `src/`, keeping only the
  Tauri command/dispatch shell.
- FE: one generic `Studio.svelte` + 5 thin wrappers (~90% reuse) replacing 5
  fat modals.
- Behavior identical, verified by the per-crate unit suite + the human's manual
  per-format round-trip / F12 / F13 pass.
