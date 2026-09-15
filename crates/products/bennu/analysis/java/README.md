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
//   FileSymbols { package, package_span, imports, types }
//   TypeDecl    { span, name, fqn, kind, is_anonymous, is_abstract, is_final, is_sealed, methods, fields, extends, implements }
//   TypeKind    = Class | Interface | Enum | Record | Annotation
//   MethodDecl  { span, name, return_type_text, params, is_static, is_abstract, is_default, is_final }
//   FieldDecl   { span, name, type_text, is_static, is_final }
// `kind` + the class modifiers + method `is_abstract`/`is_default` (a bodyless interface method is
// implicitly abstract) are what `bennu-intel` maps into the seam `ClassFlags`/`Member` flags for
// PROJECT types, so the inheritance / implement-abstract checks fire against project supertypes too.
//
// `span: Option<Span>` is BYTE offsets, and `None` means **nobody wrote it** — a record's
// accessors and canonical constructor, its `Object` overrides, a Lombok getter. `0..0` would have
// pointed at the package declaration; anything navigating to a member has to tell the two apart.

// The AST: the same parse in Java's vocabulary, bodies included, typed where the resolver can say.
fn lower_ast(source: &str, resolver: Option<&dyn TypeResolver>) -> AstNode
//   AstNode { kind, role, label, modifiers, type_name, names_a_type, synthesized, span, children }
// Derived on demand and NEVER stored — which is what makes it safe to have beside `extract_symbols`
// rather than a second model to keep in sync. Four differences from the CST, and all four are the
// point: punctuation gone; wrappers unwrapped (`expression_statement`, `parenthesized_expression`);
// Java's words rather than the grammar's (`call`, `local variable`); and every child carries the
// ROLE it plays (`condition`, `receiver`, `argument`, `returns`) — plus the two things a parse tree
// cannot hold, the resolved type and whether a bare name denotes a class or a value.
// A kind the lowering has no entry for keeps its grammar name and its children: never wrong, only
// less pretty, which is the right failure for a table over someone else's grammar.

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
// `infer_receiver_type` repairs a trailing `recv.` with a call stub, and — only when that does not
// parse and the rest of the line is blank — with the same stub finished by `;`.

// The shape of the functional interface the expression at a caret is passed to: `opt.map(|)`,
// `opt.map(Foo::|)`, a declared variable's initializer, a `return`. Params come back with the
// receiver's generics substituted (`Optional<Foo>.map` → `[Foo]`); an unbound variable is `Object`.
fn functional_descriptor_at(source: &str, byte_offset: usize, resolver: &dyn TypeResolver)
    -> Option<FunctionalDescriptor>   // { params: Vec<TypeRef>, returns: TypeRef }

// New-file scaffolding: infer a Java package from a target dir + render initial content.
fn infer_package(dir: &Path) -> Option<String>          // ".../src/main/java/com/x" -> "com.x"
fn scaffold_new_file(kind: NewFileKind, dir: &Path, name: &str) -> ScaffoldResult

// Declaration-site CST scans (go-to-declaration / rename / inherited-members consume these).
fn find_type_name_span(source: &str, simple: &str) -> Option<(usize, usize)>   // NAME token of a type decl
fn binary_of_type_at(source: &str, simple: &str, line: i64) -> Option<String>  // JVM binary name by (name, line)
// By BINARY name, nesting included: `p/Outer$Inner` and `p/Outer/Inner` are `Inner` inside `Outer`,
// never a same-named type in another outer (or a top-level namesake) of the same file.
fn find_type_declaration(root, bytes, binary: &str) -> Option<Node>          // package-exact, then longest path
fn find_binary_type_name_span(source: &str, binary: &str) -> Option<(usize, usize)>
fn declared_type_binary(decl, bytes, package) -> Option<String>              // `p/Outer/Inner` as the file spells it
fn binary_simple_name(binary: &str) -> &str                                  // innermost name of either spelling

// Overload declarations: which of a name's declarations takes a member's parameters. ONE matcher,
// shared by go-to / rename (`bennu-intel`) and library Javadoc (`FileDocs::method_overload`).
fn declared_parameter_shapes(decl, source) -> Option<Vec<ParamShape>>        // (simple type, array depth)
fn choose_overload(declarations: &[Option<&[ParamShape]>], params: &[TypeRef]) -> Option<usize>

// Javadoc: the `/** … */` above a declaration, cleaned of its markers and gutter.
fn leading_javadoc(source: &str, decl_start: usize) -> Option<String>   // for ONE offset
fn javadoc_declarations(source: &str) -> FileDocs                       // for a WHOLE file
//   FileDocs { type_doc, types: {simple name}, methods: {(name, arity)}, fields: {name},
//              overloads: {name → [OverloadDoc { parameters, doc }]} }
//   `method_overload(name, params, arity)` answers with the doc of the declaration taking `params`.
// The file-at-once form is how a LIBRARY's documentation is read: a `.class` has no comments, so
// the only copy is in its `-sources.jar` (or the JDK's `src.zip`), and one parse of that entry
// answers every hover into the type instead of re-reading the archive per pointer move. Overloads
// that agree on arity are dropped rather than merged — the doc of the wrong overload is worse
// than none.

// Written type names — one reading of `Foo` / `Outer.Inner` / `a.b.C[]` for the whole workspace.
fn resolve_written_type(text: &str, scope: &dyn NameScope) -> TypeName   // Resolved(binary) | Unknown(text)
fn same_binary_type(a: &str, b: &str) -> bool             // `Map$Entry` and `Map/Entry` are one type
fn java_lang_implicit(name: &str) -> Option<String>       // the implicitly imported package (JLS §7.3)
fn scope_candidates(simple, package, imports) -> Vec<ScopeCandidate>   // { binary, kind: ScopeKind }
//   THE statement of Java's scoping order — every other name lookup in the workspace is built on it.
//   `ScopeKind` says which route a candidate came from (SingleImport, OwnPackage, StaticImport,
//   OnDemand, StaticOnDemand, JavaLang), so a caller that cannot use a tier drops it by name —
//   `IndexResolver` is not told the file's package — instead of writing its own loop over the imports.
//   `ScopeCandidate::confirmed_by(exists)` is the one rule for when a candidate binds.
fn bind_simple_name(simple, package, imports, exists: &dyn Fn(&str) -> bool) -> Option<String>
//   What a simple name MEANS in a compilation unit, in JLS §6.4.1 precedence: a single-type import
//   (bound without asking), the file's own package, a static import of a nested type, the
//   imports-on-demand, `java.lang`. The first candidate `exists` confirms wins; `None` means nothing
//   in this file's scope binds the name — a caller that wants a project-wide guess makes it AFTER.
fn simple_name_reaches(binary, simple, package, imports) -> bool
//   Whether a compilation unit could NAME that binary by that simple name: a single-type import,
//   an import-on-demand of its package, its own package, or `java.lang`. The scoping question
//   (JLS §6.5.5 / §7.5), which a resolver deliberately does not ask — its index is keyed by simple
//   name, so it finds a type in any package, which is right for completion and wrong for a
//   "cannot resolve". Only a validator needs this; it does NOT know about types declared in the
//   file, type parameters or inherited member types, which its caller must exclude first.
```

Postfix templates — the catalogue, pure; the provider (`bennu-intel`) infers the type and makes edits:

```rust
fn postfix_subject_start(source: &str, dot: usize) -> Option<usize>     // where `expr.` begins (scan, not parse)
fn postfix_shape(ty: &TypeRef, expr: &str, resolver: &dyn TypeResolver) -> PostfixShape
//   what the type makes possible: element types (array / Iterable), Optional kind, length, closeable,
//   switchable, … plus `repeatable` (no call, no `new`) and `non_null` (literal, `this`, `new X()`);
//   `shape.respell(|name| …)` renames every name a template would declare (the caller owns conventions)
fn declared_variable_names(source: &str) -> Vec<&str>  // every field/local/parameter name, from tokens
fn postfix_expansions(subject: &PostfixSubject, ctx: &PostfixContext, wanted: &dyn Fn(&str) -> bool)
    -> Vec<PostfixExpansion>   // { name, detail, text, stops: Vec<PostfixStop { start, end, group }>, imports }
fn postfix_indent_unit(source: &str) -> String                           // the file's indentation step
//   PostfixContext { level, unit } — the level decides the form: `var` from 10, type patterns from 16,
//   `ifPresentOrElse` from 9, with the older equivalent written below; an expression that does work is
//   never written twice (declared into a local first, or the template is not offered).
```

Variable names — what a declaration of a type is called, IntelliJ's way:

```rust
fn suggested_name_for_type(written: &str) -> Option<String>
//   `URLBuilder` → urlBuilder, `List<Order>` / `Order[]` / `Order...` → orders, `Optional<Order>` → order,
//   `Map<K, V>` → map, `Class<?>` → clazz, `String` → s. The word rules are the postfix templates' own.
fn declaration_name_at(source: &str, offset: usize, case_sensitive: bool,
                       spell: impl FnOnce(String, &NameContext) -> String) -> Option<DeclarationName>
//   `spell` gets the camelCase name plus NameContext { kind: Field|Local|Parameter, declared_in_file }
//   and returns it in the project's spelling (the caller owns the conventions); the digit is added after.
//   DeclarationName { name, typed_start, typed_end } — the name for the declaration whose type ends
//   before the caret (field after modifiers, local at a statement start, method/constructor/record/
//   `for`/`try` parameter), with a digit when the name is taken in that scope (`order1`). Read from
//   tokens, not the parse. `None` after `return`/`new`/`throw`/`case`/an operator, in arguments, type
//   arguments, strings and comments, after an annotation, in a type header, for an enum constant.
//   A partial name typed after the type must be continued by the prediction; it is what accepting replaces.
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

- **Depth-capped** — `infer_expr` recurses over the expression tree, and that tree's depth is
  whatever the source says: a generated `"a" + "b" + …` of a few thousand pieces nests one level
  per piece. Past `MAX_INFER_DEPTH` (128) inference answers `None` instead of descending further.
  Exceeding it is not a stack that grows slowly — a stack overflow in Rust aborts the process,
  which in `bennu-be` means every file loses its diagnostics, so the cap is load-bearing rather
  than tidy. Hand-written code does not come close: a long fluent chain is tens of levels.

- **Overload resolution is conservative, not generic-aware** — one implementation
  (`infer/overload.rs`) serves inference, lambda-parameter typing, `call_overload_at` (what
  go-to and hover use to pick a declaration), `bound_overload` (the same answer for a call node
  and candidate set in hand — parameter hints, the signature strip) and `overload_fit` (the full
  verdict, `NoArity` / `Applicable` / `Inapplicable`, for the argument-type check, which must tell
  "ambiguous" from "nothing applies"; `arity_admits` is the count rule alone; `subtype_verdict` is
  its subtype question, for that check's per-position judgement — a hole in the argument's hierarchy
  still answers "no" for a `final` class, or for a class its readable superclass chain misses): arity (varargs-aware), then the strict / loose /
  varargs phases (a lambda fits only a functional interface whose SAM takes as many parameters,
  a method reference any functional interface, `null` any reference; widening, boxing and
  subtyping for typed arguments), then most-specific judged only at typed positions. Anything
  the classpath cannot answer abstains and keeps the candidate; a still-ambiguous overload
  resolves to "unknown" rather than a guess. Covariant overrides collapse to their derived
  return. A method's own type variables are not inferred for applicability.
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
