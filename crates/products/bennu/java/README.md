# bennu-java

The Bennu **Java source model**: parse `.java` with tree-sitter-java, extract
symbols, and — the hard, homegrown piece (docs §10) — do **local type-inference**
good enough for member-access autocomplete. Spike B said GO for homegrown: nominal
type-walks over the bytecode member index (`bennu-classpath`), not compiler-grade
inference.

## Public API (via the [`prelude`](src/prelude.rs))

```rust
// Structural model of one file.
fn extract_symbols(source: &str) -> FileSymbols
//   FileSymbols { package, imports, types }
//   TypeDecl    { name, fqn, kind, is_abstract, is_final, is_sealed, methods, fields, extends, implements }
//   TypeKind    = Class | Interface | Enum | Record | Annotation
//   MethodDecl  { name, return_type_text, params, is_static, is_abstract, is_default, is_final }
//   FieldDecl   { name, type_text, is_static, is_final }
// `kind` + the class modifiers + method `is_abstract`/`is_default` (a bodyless interface method is
// implicitly abstract) are what `bennu-intel` maps into the seam `ClassFlags`/`Member` flags for
// PROJECT types, so the inheritance / implement-abstract checks fire against project supertypes too.

// Static type of the expression immediately LEFT of the `.` at `byte_offset`.
fn infer_receiver_type(source: &str, byte_offset: usize, resolver: &dyn TypeResolver)
    -> Option<TypeRef>
// Static type of a WHOLE expression spanning [start, end) — an assigned / returned / cast value.
// Now types literals (String / int / long / double / char / boolean) and string concatenation
// (`"x" + n` → String), so the checks can catch String↔primitive mismatches.
// `_at` variants (reuse a parsed root + extracted symbols) exist for both — validation MUST use them
// (per-site re-parsing was quadratic).
//
// For the HOT path (validation over a whole file), use the `InferCache`-backed variants:
//   fn infer_node_type_cached(root, source, symbols, node, resolver, &InferCache) -> Option<TypeRef>
//   fn infer_receiver_type_cached(...) / infer_expression_type_cached(...)
// One `InferCache` per file memoizes each site's result AND each scope's locals, so a dozen checks
// that infer the same sites pay once and local resolution isn't a per-site re-scan. `infer_node_*`
// takes an ALREADY-located node (the check found it while walking), skipping the descendant search.
fn infer_expression_type(source: &str, start: usize, end: usize, resolver: &dyn TypeResolver)
    -> Option<TypeRef>

// New-file scaffolding: infer a Java package from a target dir + render initial content.
fn infer_package(dir: &Path) -> Option<String>          // ".../src/main/java/com/x" -> "com.x"
fn scaffold_new_file(kind: NewFileKind, dir: &Path, name: &str) -> ScaffoldResult

// Declaration-site CST scans (go-to-declaration / rename / inherited-members consume these).
fn find_type_name_span(source: &str, simple: &str) -> Option<(usize, usize)>   // NAME token of a type decl
fn binary_of_type_at(source: &str, simple: &str, line: i64) -> Option<String>  // JVM binary name by (name, line)
```

> The Alt+Enter **intention** transforms (parameterize logging, NP-safe equals) used to live here;
> they now have their own zero-dep crate, [`bennu-intentions`](../intentions).

`TypeResolver` is the seam the walk consumes (the caller backs it with
`bennu-classpath` + the project source index):

```rust
struct TypeRef { binary_name: String, type_args: Vec<TypeRef> }   // carries generics (caveat C2)
trait TypeResolver {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers>;                    // "java/util/ArrayList"
    fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String>;
    fn is_project_type(&self, binary_name: &str) -> bool { true }                        // project source vs JDK/dep jar
}
struct ClassMembers { superclass: Option<String>, interfaces: Vec<String>, methods: Vec<Member>, fields: Vec<Member>, flags: ClassFlags, type_params: Vec<String> }  // type_params: declared generic names, e.g. Map<K,V> → ["K","V"]
struct Member { name, kind: MemberKind, return_type: TypeRef, params: Vec<TypeRef>, is_static, is_abstract, is_default, is_final, visibility, raw_signature }
struct ClassFlags { is_interface, is_abstract, is_final, is_enum, is_annotation, is_record, is_sealed }  // decoded from bytecode
```

`TypeRef` / `ClassMembers` / `Member` are a minimal, local copy of the shared Bennu
seam — `bennu-classpath` produces the same shape from bytecode and `bennu-intel`
unifies the two at the boundary (so `bennu-java` depends only on the `TypeResolver`
trait, not on any concrete member index).

## Inference scope (Phase 1)

Handled (nominal walks, per Spike B):

- **Local variables** by declared type (incl. `Foo x = ...`) and **method
  parameters**.
- **Every binder that may legally shadow a field**, because a bare name means the
  binding, not the field: enhanced-`for` variables (`for (Foo x : xs)`, with `var`
  taking the element type), classic `for` inits, `catch` parameters (a multi-catch
  union stays unresolved rather than picking an alternative), try-with-resources,
  lambda parameters, and **pattern variables** (`o instanceof Foo f`, `case Foo f`).
  A pattern variable is bound only where Java definitely binds it — the branch its
  test governs, the rest of an `&&`, the statements after an `if (!(o instanceof Foo
  f)) return;` guard, a `switch` case body — so the name still means the field
  everywhere else.
- **`this` / `this.field`** field types, and **bare field** access (implicit `this`).
- **Method-return-type chaining**: `a.getB().getC()`.
- **Generics carry-through** (caveat C2): a `List<Foo>` local resolves `.get(i)` and
  `.iterator().next()` element type to `Foo`; a `Map<K,V>` resolves `.get(k)` to `V`.
  Substitution is a shallow one-hop heuristic on single-uppercase-letter type
  variables (`E`/`T`/`K`/`V`).
- **Casts** `(Foo) x`, **parenthesised** expressions, and **`new Foo(...)`**.
- Simple type names resolve to binary names via imports → **types declared in the
  same file** (their extracted FQN is authoritative, so a local of a same-file /
  freshly-added type resolves even before the resolver is seeded) → `TypeResolver`
  → `java.lang` fallback.
- **Inherited members**: the walk follows `superclass` + `interfaces` from
  `ClassMembers`.

A trailing-dot caret (`expr.<caret>`) is repaired by splicing a dummy call so the
buffer parses cleanly (standard completion trick; a real editor usually already has
a partial identifier).

## NOT handled yet (honest edges)

- **Overload resolution is arity-first, not full argument-subtype** — among same-named
  overloads we keep those whose arity admits the call and take their return type when it
  is unique (breaking a return-type tie by a conservative primitive/reference argument
  check); a still-ambiguous overload resolves to "unknown" rather than a guess. Covariant
  overrides collapse to their derived return. Full argument-subtype selection (boxing,
  varargs element types, most-specific) is not modelled.
- **No flow-typing / reassignment / narrowing** — a variable's declared type is used
  even after `x = somethingElse`, and `if (o instanceof Foo) { o.… }` does not narrow
  `o` itself (only a pattern variable of its own — `instanceof Foo f` — is typed).
- **`var` inference** only follows the initializer through the same expression rules;
  it does not re-run full flow analysis.
- **No raw-array element inference** (`arr[i].`) — only generic collections carry
  through.
- **No static member access on a bare type name** (`Collections.` /
  `Integer.MAX_VALUE`) — the receiver must resolve to an instance/value type.
- **Generics substitution is shallow**: nested/bounded/wildcard type variables beyond
  the one-hop `E`/`T`/`K`/`V` heuristic are not fully substituted; declared bounds are
  ignored.
- **No cross-file resolution inside the walk** beyond what the `TypeResolver` returns;
  same-file source types are resolved directly, everything else is delegated.

Depends on `bennu-classpath` (the member index the type-walk resolves against).
