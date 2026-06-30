# Studio → `crates/wasm/studio/` — Extraction & Genericization Plan

Status: **PLAN ONLY — not executed.** This is the blueprint for moving the Studio
subsystem out of the launcher into dedicated crates and collapsing its
per-format duplication into a shared core.

## Why this is a plan, not a finished migration

Studio is **large and data-integrity-critical**:

- ~14k LOC Rust (5 formats × ~2.5k each: `<fmt>_studio/mod.rs` + `backend_impl.rs`),
  plus the `studio/` scanner + trait/registry (~1.7k).
- ~13k LOC frontend (5 per-format modals + the generic `studio/` UI & composables).
- The formats do **lossless editing of real on-disk config files** with undo/redo,
  JSONPath queries, cross-ref indexing, F12 rename-refactor and F13 bulk-edit.

A silent bug in a lossless editor = **corruption of the user's config files**.
`cargo check` green is necessary but **not sufficient** — correctness here needs
per-format round-trip testing that cannot be validated by the agent (the app is
never run in this workflow). So the extraction must run in a **supervised session**
where each format can be manually exercised. The discovery agent's own estimate:
**2–3 weeks, ~20–30% codebase reduction.**

## Current architecture (grounded)

Registry + trait dispatch:

- `src-tauri/src/studio/format/backend.rs` — `StudioFormatBackend` trait (async
  `parse`/`close`, tree nav, `query`, mutations, undo/redo, diff, save, schema;
  optional F12 `rename_preview/apply`, F13 `bulk_edit_preview/apply`).
- `src-tauri/src/studio/format/registry.rs` — `HashMap<format_id, Arc<dyn StudioFormatBackend>>`,
  populated at startup.
- `src-tauri/src/studio/format/descriptor.rs` — `FormatDescriptor` capability flags
  (mirrored by the FE to gate affordances).
- `src-tauri/src/studio/mod.rs` (~1272) — format-agnostic repo scanner: cross-ref
  defs/usages/broken-refs, delegating to per-format AST visitors
  (`collect_defs_ron_into`, `collect_defs_json_into`, …).
- Per format `<fmt>_studio/mod.rs` (registry + AST + ops) and `<fmt>_studio/backend_impl.rs`
  (trait impl + legacy-type↔trait-type mapping): `ron`, `json`, `toml`, `yaml`, `properties`.

Frontend:

- Generic shell already exists under `src/lib/components/shared/studio/`
  (`StudioModal`, `StudioTreePane`, `StudioTextPane`, `StudioDiffPane`,
  `StudioInspectorPanel`, `StudioQueryBar`, `StudioSchemaPanel`, `StudioRefsPanel`,
  `StudioRenameModal`, `StudioBulkEditModal`) + 11 `useStudio*` composables.
- Per-format modals (`RonStudioModal` 2545, `JsonStudioModal` 1560, `YamlStudioModal`
  1483, `TomlStudioModal` 1357, `PropertiesStudioModal` 1331) — 60–70% of each is
  format-specific wiring, the rest duplicated.
- IPC: `src/lib/ipc/studio-format.ts` (unified backend wrapper), `studio.ts` (sidebar
  index), `studio-convert.ts` (cross-format codec).

## Target crate layout

```
crates/wasm/studio/
├── types/        arbor-studio-types   — shared DTOs (NodeView, QueryHit, DiffHunk,
│                                         FormatDescriptor, ParseResult, MutateResult,
│                                         RenamePreview, BulkEditPreview, Sites…). NO logic.
├── core/         arbor-studio-core    — the StudioFormatBackend trait + the generic,
│                                         format-agnostic logic that today is copy-pasted:
│                                         History<T> (coalescing/cursor/cap), unified+tree
│                                         diff over serde_json::Value, JSONPath query engine,
│                                         F12/F13 site enumeration, the cross-ref scanner.
├── api/          arbor-studio-api     — the host-facing facade: the registry, the
│                                         dispatch entrypoint a host (launcher now, wasm
│                                         guest later) calls. Tauri-free.
├── ron/          arbor-studio-ron     — RON AST + registry + backend impl (owns syn,
│                                         prettyplease, quote for the .rs schema feature)
├── json/         arbor-studio-json    — JSON/JSONC (owns simd-json, jsonc-parser, serde_json_path)
├── toml/         arbor-studio-toml    — TOML (owns toml_edit)
├── yaml/         arbor-studio-yaml    — YAML (owns yaml-edit, serde_yaml_ng)
└── properties/   arbor-studio-properties — .properties (line-based)
```

Naming follows the reorg rule: under `wasm/`, crates are `arbor-*`.
Each per-format crate exposes `pub fn backend() -> Arc<dyn StudioFormatBackend>`
and a `collect_defs_into(...)` for the scanner.

The launcher keeps only **thin glue**: the Tauri command layer that binds
`format_id` and forwards to `arbor-studio-api`. Same shape the existing
`studio(method, params)` IPC dispatcher uses — so the FE is unaffected.

## Genericization — the ~10k LOC to delete

| Today (duplicated ×5) | Moves to |
|---|---|
| legacy-type ↔ trait-type mapping in each `backend_impl.rs` (~30–50% of each) | a generic `DefaultBackend<T>` wrapper in `core` for the simple formats (TOML/YAML/.properties) |
| history/undo-redo coalescing (~100–150 LOC ×5) | `History<T>` generic in `core` |
| unified diff + tree diff (×5) | `diff::{unified, tree}(orig, cur: &Value)` in `core` |
| JSONPath query execution (×5) | `query::run(root: &Value, expr)` in `core` (all formats project to `Value`) |
| F12/F13 site builders (×5) | `refactor::{rename_sites, bulk_sites}` in `core`; backends provide `apply_string_rename` / `apply_bulk_ops` (lossless, per-format) |
| FE: `useStudioGlobalKeys` + undo/redo + edit pipeline instantiated per modal | instantiate once in a generic `Studio.svelte`; per-format modals shrink to 50–100 LOC wrappers |

Formats that **keep** hand-written backends (too special for `DefaultBackend<T>`):
RON (inline `.rs` schema introspection) and JSON (dual AST: simd-json read path +
jsonc-parser edit path, stream mode).

## Staged execution (each stage ends green; manual round-trip test per format)

0. **Coupling audit** (1st step of execution): grep every `crate::` symbol the
   `studio`/`*_studio` modules reference from the launcher (error types, encoding
   helpers, AppState). Anything shared must move to `types`/`core` or be re-imported.
   This sizes the real effort precisely.
1. **`types` + `core` skeleton**: create the two crates; move the trait + DTOs +
   descriptor into them. Launcher's per-format backends now `impl` the crate trait.
   Compile. (No behavior change.)
2. **Lift the generics into `core`** one at a time (History, diff, query, refactor),
   rewiring all 5 backends to call them. **Compile + manual test after each.** This
   is the risky, behavior-sensitive part — do it generic-by-generic, never all at once.
3. **Per-format crates**: move `<fmt>_studio/*` into `arbor-studio-{fmt}`, each owning
   its format libs. Move the studio-only deps out of `src-tauri/Cargo.toml` into the
   crate that needs them (see T3 below). Compile + test per format.
4. **`api` facade + launcher glue**: move the registry/dispatch into `arbor-studio-api`;
   launcher keeps only the Tauri command shells. Compile + smoke all formats.
5. **Frontend**: collapse the 5 per-format modals onto a generic `Studio.svelte`;
   per-format wrappers become thin. `npm run check`. Manual UI pass per format.

## T3 — launcher deps freed once studio leaves

These are **studio-only** in `src-tauri/Cargo.toml` (confirmed by discovery) and move
into the per-format crates, then get deleted from the launcher:

- `simd-json`, `jsonc-parser`, `serde_json_path` → `arbor-studio-json` (query also used by all → lives in `core`, pulling `serde_json_path` there)
- `syn`, `prettyplease`, `quote` → `arbor-studio-ron`
- `toml_edit` → `arbor-studio-toml`
- `yaml-edit`, `serde_yaml_ng` → `arbor-studio-yaml`

(`toml`, `serde_json` etc. stay — used elsewhere in the shell.)
T3 is therefore **blocked on T2 stage 3+**: the deps can only leave once the code using
them leaves.

## Risks / validation requirements

- **Lossless correctness is not compile-checkable.** Every format needs a manual
  round-trip test (open → edit → undo/redo → save → diff byte-identical except intent)
  before its stage is considered done. Comments/anchors/quote-style/variant-tags must survive.
- **F12/F13 across files** touch multiple on-disk files — test rename + bulk-edit on a
  sample repo, verify collision detection + dirty-blocker still gate.
- **Cross-ref scanner** must keep delegating to every format's `collect_defs_into`.
- Do stages behind a branch; keep each stage individually revertible.

## Relationship to WASM

This extraction is also **step 1 of making Studio a WASM plugin** (it's pure-CPU, no
network — the easiest of cloud/brp/studio to target wasm32; see `docs/wasm-plugin-integration.md`).
The Tauri-free `core`/`api`/per-format crates are exactly what a wasm guest would compile.
Doing the extraction first (in-process, behavior-preserving) de-risks the later wasm step.

See [[project_crate_reorg]], [[project_studio_extraction_analysis]].
