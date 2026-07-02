# bennu-intel

The Bennu **code-intel provider seam** — **Phase-0 skeleton**.

The heart of the abstraction (docs §2): the FE speaks **one** protocol for every
language via the `IntelProvider` trait (completion / hover / definition / references
/ diagnostics / rename / format / symbols). Java goes to the native, index-backed
engine; Rust *will* go to rust-analyzer via LSP — the "predisposed LSP" the design
requires.

Two impl slots:

- **`NativeJavaProvider`** — the MVP impl, index-backed. **Phase 1 implements
  member-access completion end to end**: it holds an `IndexResolver` over the built
  project index (`bennu-index`) + the JDK member index (`bennu-classpath`), infers the
  receiver type at the caret (`bennu-java`), walks its members (superclass + interfaces),
  and prefix-filters into `CompletionItem`s. Constructed empty via `new()` (before a
  project is open / while the index is still building), it answers completion with the
  benign empty list. Hover / definition / references / rename / format stay stubbed.
- **`LspClientProvider`** — the **predisposed** rust-analyzer slot. Documented and
  present, **not implemented in the MVP** (tower-lsp deferred — docs §4); its methods
  return `IntelError::Unimplemented`. Later LSP wiring is a fill-in of these bodies,
  not a new shape.

## Phase-1 completion machinery

- **`java_index`** — turns a project's `.java` sources into `bennu-index` `IndexRecord`s
  (each type a `Class` symbol whose `members_json` is its resolved member surface).
- **`resolver`** — `IndexResolver`, the `bennu-java` `TypeResolver` over the persisted
  project index + JDK member index. Converts `bennu-classpath`'s member shape into the
  `bennu-java` seam shape at the boundary (`convert_members`).
- **`jdk`** — `JdkMemberIndex`, a mutex-serialized `Send + Sync` wrapper around the boxed
  JDK classpath source (the JDK-8 `JarSource` is `!Sync`; the mutex restores `Sync` so the
  provider can live in the multi-threaded backend state).
- **`completion`** — the caret → candidates query.

## Config-graph integration (Struts / Spring / Tiles)

- **`config`** — ingests the `bennu-web` config-graph into the index and resolves the
  load-bearing chains off it:
  - `ingest_config_graph(&graph, index_dir)` assigns `u32` ids to the string-keyed
    action/bean records (`Source::StrutsAction` / `SpringBean` symbols) and writes the
    resolvable edges to the relation store, returning a `ConfigResolver`.
  - `ConfigResolver::resolve_action_class(action)` — the **C1 chain**: action → Spring
    bean-id → real FQCN, over the ingested `ActionToClass` edge (+ the Spring parent
    chain for a class-less bean).
  - `ConfigResolver::resolve_action_view(action)` — action → `<result type=tiles>` →
    Tiles def → JSP.
  - `ConfigResolver::diagnose_action(action)` → `ActionVerdict::{Exists, Missing,
    Inconclusive}` — the conservative "action inesistente" check: a wildcard candidate or
    a computed/OGNL path is **Inconclusive**, never a false **Missing** (docs §7/§8).
  - `ConfigResolver::action_class_ref(action)` → `ActionTarget` (config fragment + class
    FQCN + view JSP) for go-to-definition.

## References + rename (docs §5 #7, #10-12)

- **`refs`** — the cross-file reference index + the caret classifier both find-usages and
  rename key off:
  - `build_reference_index(files, resolver, project_types)` walks every use site in the
    project (method invocation / field access / type reference), resolves each to its
    declaring `DeclKey` (`Type` / `Method` / `Field`) via receiver inference + the
    supertype walk, and buckets the `UsageLocation`s by declaration. Unresolved sites are
    skipped, never fatal.
  - `classify_caret(...)` → the `DeclKey` a caret references (declaration site or use
    site). `classify_target(...)` is the rename superset that also recognises a **local
    variable / parameter** (`RenameTarget::Local`), which find-usages doesn't bucket.
- **`rename`** — best-effort, preview-first rename planning (docs §5 #10-12):
  - `RenameEngine::for_project(index_dir, jdk, simple_names, java_sources, xml_sources)`
    opens the persisted index, builds the resolver + the reference index, and caches the
    source sets — one `Send + Sync` engine per project (built off-thread).
  - `engine.plan(file, source, offset, new_name)` → a `RenamePlan` PREVIEW: per-file
    `Edit`s tagged by `EditReason` (`Declaration` / `Reference` / `Import` / `SpringBean` /
    `Local`) with an `inferred` flag. A **local** is scope-exact single-file; a
    **method/field** is decl + cross-file refs (method refs `inferred` — overloads collapse
    to one key); a **class** is decl + refs + `import`s + Spring `<bean class="FQCN">`
    (`bennu_web::bean_class_value_spans` — a Struts `<action class="beanId">` is a bean-id,
    not the FQCN, so it is NOT edited). `rename_apply(&plan)` flattens to the edits the FE
    applies.

## Usage

```rust
use bennu_intel::prelude::*;
```
