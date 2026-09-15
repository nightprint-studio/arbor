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
the `.`, walk its members (super + interfaces), filter by the typed prefix and by what can be written
from the caret's class (private / package-private / protected, JLS §6.6.2), return
`Vec<CompletionItem>`. A member list is ordered by expected-type `Fit`, then match tier, then
`MemberOrigin` (own > inherited > `Object`; a record's implicit `equals`/`hashCode`/`toString` count as
inherited), then relevance; overloads stay separate rows (fewest parameters first — only a `::` list
folds them), and with nothing typed the first own member is preselected. Each member item carries
`signature` (`(String prefix, int limit)`), `detail` = the return type (`getClass()` typed
`Class<? extends Receiver>`), `member_origin` and `modifiers`.

`dep_record` powers the **incremental validation cache**: a `record(|| …)` scope captures every
project type a validation reads through the resolver (`members_of` / `resolve_simple_name` are the
single choke point) — types whose members were read, bare names that resolved to a project type,
and names probed and found *absent* (negative deps). The `ProjectView` trait (impl'd by
`IndexResolver`, via `dep_signature` / `project_simple` / `project_contains`) re-checks those
recorded deps against the live project, so `bennu-intel`'s diagnostic cache can tell — with no false
positives — when a cached diagnostic list is still valid. Recording is a thread-local, gated by a
cheap flag, so it's a no-op on the hot completion / reference-walk paths.

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
trait ProjectView                           // read-only project view for cache freshness checks
struct RecordedDeps                          // project deps captured during a validation
fn record(f) -> (R, RecordedDeps)            // run `f` capturing the project types it reads
fn convert_members(&CpClassMembers) -> JClassMembers
fn completion(source: &str, byte_offset: usize, resolver: &IndexResolver<M>) -> Vec<CompletionItem>
fn completion_in(source, byte_offset, resolver, catalog: Option<&dyn TypeNameCatalog>, case) -> Vec<CompletionItem>
// After `::` (`Type::|`, `value::na|`) `completion_in` answers the method reference instead: methods
// and `new` only, inserting the bare name, the ones fitting the slot's functional interface first.
fn method_reference_site(source: &str, caret: usize) -> Option<MethodReferenceSite>   // { qualifier_end, name_start }
fn method_reference_completion(source, caret, resolver, catalog, case) -> Option<Vec<CompletionItem>>
struct ReferenceShape { params, returns, qualifier, through_type }
fn fits_reference(m: &Member, shape: &ReferenceShape) -> bool
fn params_fit(declared: &[TypeRef], wanted: &[TypeRef]) -> bool
// How a candidate answers the type the position wants (`return builder.|`, `String s = o.|`):
// None < Subtype < Exact. Every completion list is ordered by it FIRST, then by relevance; the
// only exact fit is marked `preselect`.
enum Fit { None, Subtype, Exact }
// Where a member stands relative to the receiver (bennu_proto::MemberOrigin: Object < Inherited < Own).
fn member_origin(m: &Member, declaring: &str, depth: usize, declared: &ClassMembers) -> MemberOrigin
// Whether a `protected` member of `declaring` can be written through `receiver` from `site`; true when unsure.
fn protected_visible(resolver, declaring: &str, receiver: &TypeRef, is_static: bool, site: Option<&str>) -> bool
// The classes a function slot receives (`opt.map(Re|)` → the Optional's element), as type items.
fn functional_argument_types(source, caret, resolver, case) -> Vec<CompletionItem>
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
