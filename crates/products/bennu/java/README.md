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
//   TypeDecl    { name, fqn, methods, fields, extends, implements }
//   MethodDecl  { name, return_type_text, params, is_static }
//   FieldDecl   { name, type_text, is_static }

// Static type of the expression immediately LEFT of the `.` at `byte_offset`.
fn infer_receiver_type(source: &str, byte_offset: usize, resolver: &dyn TypeResolver)
    -> Option<TypeRef>
```

`TypeResolver` is the seam the walk consumes (the caller backs it with
`bennu-classpath` + the project source index):

```rust
struct TypeRef { binary_name: String, type_args: Vec<TypeRef> }   // carries generics (caveat C2)
trait TypeResolver {
    fn members_of(&self, binary_name: &str) -> Option<ClassMembers>;                    // "java/util/ArrayList"
    fn resolve_simple_name(&self, name: &str, imports: &[Import]) -> Option<String>;
}
struct ClassMembers { superclass: Option<String>, interfaces: Vec<String>, methods: Vec<Member>, fields: Vec<Member> }
struct Member { name, kind: MemberKind, return_type: TypeRef, params: Vec<TypeRef>, is_static, visibility, raw_signature }
```

`TypeRef` / `ClassMembers` / `Member` are a minimal, local copy of the shared Bennu
seam — `bennu-classpath` produces the same shape from bytecode and `bennu-intel`
unifies the two at the boundary (so `bennu-java` depends only on the `TypeResolver`
trait, not on any concrete member index).

## Inference scope (Phase 1)

Handled (nominal walks, per Spike B):

- **Local variables** by declared type (incl. `Foo x = ...`) and **method
  parameters**.
- **`this` / `this.field`** field types, and **bare field** access (implicit `this`).
- **Method-return-type chaining**: `a.getB().getC()`.
- **Generics carry-through** (caveat C2): a `List<Foo>` local resolves `.get(i)` and
  `.iterator().next()` element type to `Foo`; a `Map<K,V>` resolves `.get(k)` to `V`.
  Substitution is a shallow one-hop heuristic on single-uppercase-letter type
  variables (`E`/`T`/`K`/`V`).
- **Casts** `(Foo) x`, **parenthesised** expressions, and **`new Foo(...)`**.
- Simple type names resolve to binary names via imports → `TypeResolver` →
  `java.lang` fallback.
- **Inherited members**: the walk follows `superclass` + `interfaces` from
  `ClassMembers`.

A trailing-dot caret (`expr.<caret>`) is repaired by splicing a dummy call so the
buffer parses cleanly (standard completion trick; a real editor usually already has
a partial identifier).

## NOT handled yet (honest edges)

- **No overload resolution by argument types** — the first method matching by name
  wins. (`String.valueOf(...)` etc. resolve to whichever overload is listed first.)
- **No flow-typing / reassignment / narrowing** — a variable's declared type is used
  even after `x = somethingElse`; no ternary/`instanceof` narrowing.
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
