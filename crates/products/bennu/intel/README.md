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
  `project_members()` enumerates the built index's members (methods + fields) as
  `ProjectMember`s for the index inspector's members list.
- **`LspClientProvider`** — the **predisposed** rust-analyzer slot. Documented and
  present, **not implemented in the MVP** (tower-lsp deferred — docs §4); its methods
  return `IntelError::Unimplemented`. Later LSP wiring is a fill-in of these bodies,
  not a new shape.

## Phase-1 completion machinery

- **`java_index`** — turns a project's `.java` sources into `bennu-index` `IndexRecord`s
  (each type a `Class` symbol whose `members_json` is its resolved member surface). Sources
  are read through `read_source_for_index`, which decodes in the project's declared (Maven
  `sourceEncoding`) encoding via `bennu-project`'s `decode_for_index` — recovering + flagging a
  file whose bytes don't fit (through `encoding_rs`) rather than silently dropping it. So a
  non-UTF-8 legacy file (Cp1252 / ISO-8859-1) is still indexed; `read_java_sources` returns the
  non-compliant subset alongside the sources, and only a genuine IO error skips a file (logged).
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
  - `ingest_config_graph(&graph, index_dir, annotation_beans)` assigns `u32` ids to the
    string-keyed action/bean records (`Source::StrutsAction` / `SpringBean` symbols) and
    writes the resolvable edges to the relation store, returning a `ConfigResolver`. The
    `annotation_beans` (from `spring_beans::collect_annotation_beans`) seed a **separate**
    name→bean map (Option B — the pure-XML `graph.beans` stays untouched) for the C1
    fallback below.
  - `ConfigResolver::resolve_action_class(action)` — the **C1 chain**: action → Spring
    bean-id → real FQCN, over the ingested `ActionToClass` edge (+ the Spring parent
    chain for a class-less bean). When no XML `<bean>` names the id, it falls back to the
    annotation-declared beans (`@Service`/`@Component`/…) so annotation-based apps resolve
    the class too. `resolve_bean(name)` / `annotation_bean_count()` expose that map.

- **`spring_beans`** — the stereotype-bean policy: `collect_annotation_beans(sources)`
  scans the project's Java for `@Component`/`@Service`/`@Repository`/`@Controller` (+ the
  meta-stereotypes and JSR-330 markers) and reproduces the bean each declares — its name
  (explicit `value`, else the decapitalized simple name) and impl FQCN.
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
    project (method invocation / field access / type reference / **bare field read**),
    resolves each to its declaring `DeclKey` (`Type` / `Method` / `Field`) via receiver
    inference + the supertype walk, and buckets the `UsageLocation`s by declaration.
    Unresolved sites are skipped, never fatal.
  - The bare arm — a `count` standing for `this.count` — mirrors `classify_caret`'s exactly,
    because the index and the query must produce the same key or the lookup finds an empty
    bucket. It is gated on a memoized `field name → declaring type` table per enclosing type,
    which rejects nearly every identifier in a method body (they are locals) before the
    per-scope shadowing walk runs at all. When the resolver does not know the type — a class the
    index has not reached, a nested one it holds under another name — the gate falls back to the
    **file's own parsed fields**, because the query side's lookup is lenient there and would
    otherwise build a key for a bucket nothing filled.
  - **One spelling of "the enclosing type".** Both sides call the same
    `enclosing_type_binary`; the walker used to keep its own copy that asked the resolver where
    the query's falls back to the buffer's `package` line, and the two answers differed for
    exactly the types the project map does not hold. The symptom is a member with no usages that
    <kbd>Ctrl</kbd>+click navigates from correctly — a drift between an index and the query that
    reads it never announces itself.
  - `classify_caret(...)` → the `DeclKey` a caret references (declaration site or use
    site). `classify_target(...)` is the rename superset that also recognises a **local
    variable / parameter** (`RenameTarget::Local`), which find-usages doesn't bucket.
  - `build_reference_index_incremental(files, resolver, project_types, prior, on_progress)`
    is the persisted, incremental path (a full walk is just `prior = None`). The walk is a
    merge of independent per-file contributions, so it caches: see **`refcache`**.
- **`refcache`** — the persisted, incremental reference-index cache (JSON at a stable path
  under the index base, surviving per-build gen dirs). On reopen it re-walks only the files
  whose source hash changed **plus** (dependency-aware) any file whose recorded deps name a
  type declared by a changed file; a global type-set guard (`type_map_hash`) forces a full
  walk on any structural type change. So the first open pays the O(N) walk once and later
  opens are near-instant. The manual "Rebuild index" deletes the cache (`clear`) for a clean
  full walk.
- **`diag_cache`** — the persisted, dependency-aware cache of per-file **validation
  diagnostics** (same shape/location discipline as `refcache`). Each entry stores the file's
  own content hash plus the PRECISE project dependencies its validation recorded
  (`bennu_query`'s `RecordedDeps`): the types whose members it read, the bare names it
  resolved to a project type, and the names it probed and found **absent**. An entry is reused
  only while all four still hold against the live resolver (`ProjectView`), so a re-validation
  of an unchanged project (or the unchanged part of an edited one) is instant and can never
  serve a stale diagnostic — the recorded set is a superset of everything validation reads
  from the mutable project surface. A classpath/JDK `epoch` in the header drops the whole cache
  when the classpath changes; "Rebuild index" clears it (`clear`). The be layer's whole-project
  "Validate (no compile)" loads it, serves + fills it, prunes deleted files, and persists it.
- **go-to-declaration** (Ctrl+Click / Ctrl+B) — `resolve_declaration(...)` (and
  `RenameEngine::declaration(file, source, offset)`) reuse the same caret classifier +
  decl-site name-span finders (`find_member_name_span` here; the type-name finder
  `find_type_name_span` now lives in `bennu-java`, re-surfaced through this prelude) to return
  a `DeclarationLocation` (owning project file + declaration NAME span + 1-based line/col +
  label). A local/param resolves to its declarator in the current buffer; a method/field to
  its name token on the owner type; a class/interface/enum to its type-declaration name.
  A JDK / dep-jar declaration (no project source) yields `None` — nothing to open.
- **inherited members** — the inherited ("super") members of a type (Structure panel's lazy
  "Inherited" bucket) now live in `bennu-query` (`inherited_members(...)`, a pure resolver
  walk). `RenameEngine::inherited_members(file, type_name, line)` is the thin engine-scoped
  entry that forwards the engine's resolver + java sources to it.
- **`rename`** — best-effort, preview-first rename planning (docs §5 #10-12):
  - `RenameEngine::for_project(index_dir, jdk, simple_names, java_sources, xml_sources)`
    opens the persisted index, builds the resolver + the reference index (via the
    incremental `refcache` — only changed files + their dependents are re-walked on reopen),
    and caches the source sets — one `Send + Sync` engine per project (built off-thread).
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
