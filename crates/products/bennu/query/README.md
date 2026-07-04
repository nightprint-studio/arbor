# bennu-query

The Bennu code-intel **query engine**: the read-only resolver + member-access completion, split out
of `bennu-intel` so it depends only on the base crates.

`IndexResolver` implements `bennu-java`'s `TypeResolver` over two member sources:

1. the **persisted project index** (`bennu-index`) — `.java`-declared types, with an in-memory
   overlay for files edited since the last full build;
2. the **JDK / library bytecode index** (`bennu-classpath`, wrapped `Send + Sync` in
   `JdkMemberIndex`). `JdkMemberIndex` is a **persistent lazy** index: it memoizes every lookup
   (hits *and* definitive misses) and, when built with `persistent(source, path)`, loads/saves that
   memo to a JSON file **keyed by the resolved JDK** — so a JDK class is parsed from bytecode at
   most once ever, shared across projects and sessions (the be layer keys the path under
   `bennu_data_dir()/jdk-index/`). `new(source)` is the in-memory-only variant.

`completion(source, byte_offset, &resolver)` is the member-access query: infer the receiver type at
the `.`, walk its members (super + interfaces), filter by the typed prefix, return
`Vec<CompletionItem>`.

`inherited_members(&resolver, java_files, file, type_name, line)` collects a type's SUPERCLASS +
INTERFACES members (recursively, deduping overrides) — the Structure panel's lazy "Inherited"
bucket. It reuses the same `members_of` supertype walk as completion, one level up from the type's
own members. The tree-sitter CST scans it needs (resolve the target's binary name by `(simple,
line)`; locate a supertype's project source) are delegated to `bennu-java`
(`binary_of_type_at` / `find_type_name_span`), so this crate stays **parser-free** — a pure
resolver walk.

## Public API (via the [`prelude`](src/prelude.rs))

```rust
struct IndexResolver<M: MemberIndex>       // impls bennu_java::TypeResolver
struct JdkMemberIndex                       // Send + Sync, persistent lazy JDK member index
struct PlanFile { path, source }            // a project source file (whole-project query input)
struct InheritedMember / InheritedSource    // the inherited-members result shape
fn convert_members(&CpClassMembers) -> JClassMembers
fn completion(source: &str, byte_offset: usize, resolver: &IndexResolver<M>) -> Vec<CompletionItem>
fn inherited_members(resolver, java_files: &[PlanFile], file, type_name, line) -> Vec<InheritedMember>
```

## Who consumes it

`bennu-intel` builds an `IndexResolver` and uses it for **completion** (via the provider),
**find-usages**, **rename** and **hover**; **inherited-members** lives here directly and
`RenameEngine::inherited_members` is a thin engine-scoped forwarder. `PlanFile` (the project-source
input unit) is shared by inherited-members and `bennu-intel`'s rename planner. `bennu-query` is
read-only; the reference-walk (find-usages/rename) machinery stays in `bennu-intel` and depends
one-way on this crate.

Depends only on `bennu-index`, `bennu-classpath`, `bennu-java`, `bennu-proto`.
