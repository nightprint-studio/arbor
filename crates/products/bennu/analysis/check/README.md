# bennu-check

Java **validation without compiling** — the "red squiggle before you run Maven". Two tiers: pure
tree-sitter-java scans (`check_file`, no resolver) and resolver-backed checks that run type
inference (`check_file_resolved`). Both emit the wire `Diagnostic` (byte offsets) the Problems panel
+ lint gutter already render. Leaf-ish crate (depends only on `bennu-java`, `bennu-lombok` and
`bennu-proto`) — exhaustively unit-tested here, including with real type inference.

## Layout

One folder per area; each check is one module inside it.

| folder | what lives there |
|---|---|
| `engine/` | the aggregators (`check.rs`, `incremental.rs`), the `CheckId` catalog, parse-error quarantine, the inspection policy, the javac coverage table (`javac.rs` + generated `javac_table.rs`) |
| `support/` | shared readings of the tree and the resolver (`nodes`, `text`, `walk`, `resolve`, `scopes`, `supertypes`, `constant`, `lombok`, …) — no diagnostics of their own |
| `source/` | syntax errors, statements, generics syntax, imports, file/package agreement, version gates |
| `decls/` | declaration legality: modifiers, duplicates, redeclarations, constructors, initialization, unused members |
| `annotation/` | annotation targets, element declarations and values, `@FunctionalInterface` |
| `hierarchy/` | extends/implements, cycles, overrides, `super` calls, visibility, static context |
| `calls/` | unknown members/fields, argument count and types, unresolved names, lambdas and captures |
| `typing/` | unresolved types, type-argument arity, casts, narrowing, condition types |
| `flow/` | returns, reachability, final reassignment, break/continue, data flow, expression lints |
| `throwing/` | checked exceptions, try/catch legality, dead catches |
| `switching/` | switch selectors, yields, labels, enum exhaustiveness, fall-through |

## Lombok

Several checks have to stay silent about members that exist in the compiled class and nowhere in the
source. None of them owns that knowledge: it lives in `bennu-lombok` (which `bennu-intel` shares), and
`src/support/lombok.rs` is only the tree-sitter adaptation — imports and annotations out of the CST, strings
into the catalogue. Add a check that has to reason about Lombok and it goes through the same module,
so it cannot disagree with the index about what a `@Builder` class contains.

## Pure-AST checks (`check_file`, no resolver)

| Check | Emits | What |
|---|---|---|
| `syntax_errors` | `error` | tree-sitter `ERROR` / `MISSING` nodes (missing `;`/`)`, unclosed brace). Multi-line spans clamped to the first line; clean subtrees pruned. |
| `invalid_statements` | `error` | an expression JLS §14.8 forbids as a statement — a bare field access (`stepper.add;`), a lone identifier, arithmetic (`1+1;`). Catches the classic forgotten call (`list.clear;`). |
| `missing_return` | `error` | a non-`void` method whose body can complete normally (JLS §14.22 — loops with their `break`s and constant conditions, `switch` with/without `default`, `try`/`catch`/`finally`, labels), reported on the closing brace. `can_complete_normally` is shared with the switch-expression checks; a shape it does not model answers "cannot complete", and a loop condition naming something that may be a constant elsewhere counts as constant. |
| `declaration_errors` | `error` | modifier/declaration legality: `abstract` method in a concrete class (on the class), `abstract` method with a body, `default` method outside an interface, `abstract`+`final` class, `abstract`+`private`/`static`/`final`, two visibility modifiers, `final`+`volatile` field, modifiers a kind never takes (`abstract`/`final` enum, `abstract` field, `transient` method, `final`/`synchronized` interface method, `private`/`static` top-level type, `final` interface), `sealed`/`non-sealed`/`final` together, an interface method with a body and no `default`/`static`/`private`, a `record` that's `abstract` or declares instance fields or initializers, an `enum` constant with args but no constructor. |
| `record_ctor_errors` | `error` | record constructors (JLS §8.10): a canonical one — the component types in order — with other parameter names, a `throws` clause, less access than the record, or a component left unassigned (on the closing brace); a compact one that `return`s; a non-canonical one not starting with `this(…)`; an accessor that is not `public`, is `static`, throws, or returns another primitive. Types compare as written, so another spelling of the same type is left alone. |
| `sealed_errors` | `error` | sealed hierarchies inside one file: a subtype missing from `permits`, a permitted subclass not `final`/`sealed`/`non-sealed`, a sealed type with neither `permits` nor a subclass in the file, a `non-sealed` type with no sealed supertype. A supertype declared elsewhere, or a name also imported, is never judged. |
| `static_type_var_errors` | `error` | a type's own type parameter used in a static field, method, initializer or static nested type (`static T cache;`). A nearer `<T>` shadows it; a type of the same name in the file silences it. |
| `annotation_errors` | `error` | a built-in annotation on the wrong target — `@Override` off a method, `@FunctionalInterface` off an interface, `@SafeVarargs` off a method/constructor. Unknown annotations are skipped. |
| `lambda_capture_errors` | `error` | assigning / `++`/`--` a captured local inside a lambda (must be effectively final). Only enclosing-method locals are flagged, so fields are never mis-reported. |
| `class_name_matches_file` | `error` | a `public` top-level type whose name ≠ the file base name (needs `file_stem`). |
| `special_file_errors` | `error` | the restricted grammar of `package-info.java` (only a package declaration + its annotations) and `module-info.java` (only a module declaration). Keyed off `file_stem`. |
| `return_statement_errors` | `error` | a value returned from a `void` method / constructor, or a bare `return;` in a non-`void` method. Attributed to the nearest method — a `return` inside a lambda is judged against the lambda. |
| `duplicate_signatures` | `error` | two methods or two constructors in the same type with the same name and parameter types. Exact text match (generics kept) so a legal overload is never flagged. |
| `redeclaration_errors` | `error` | the same name declared twice where Java forbids it — two fields in a type, two parameters of a method/constructor/lambda, two locals in one block, or two types with the same name in one scope. Exact-name, same-scope, so a legal same-name in a disjoint sibling scope (two `for` loops each declaring `i`) or a local shadowing a field is never flagged. |
| `unreachable_code` | `error` | a statement directly after an unconditional `return`/`throw`/`break`/`continue` in the same block. Conservative — a terminator nested in an `if` doesn't kill the following code; only the first unreachable statement per block is reported. |
| `switch_yield_errors` | `error` | a `switch` **expression** arrow block that can complete normally, a LAST colon group that can (earlier groups may fall through), an expression with no case, or one whose every arm throws. Only in value positions. |
| `constant_switch_exhaustiveness` | `error` | a `switch` expression whose labels are all literals (`int`/`char`/`String`) and that has no `default` — never exhaustive, whatever the selector. |
| `switch_selector_errors` | `error` | a `switch` on a `long` / `float` / `double` / `boolean` — types `switch` doesn't accept — and, on an `int`/`short`/`byte`/`char` selector, a string label or a decimal label outside the type's range (`case 200` on a `byte`). Purely syntactic (declared type / literal), so no resolver needed. |
| `final_reassignment_errors` | `error` | assigning something `final`: a `final` local with an initializer, a `final` parameter, a `final` or multi-`catch` parameter, a `final` for-each variable, and a `final` field — through `this.` or an unshadowed bare name — when it has an initializer, when it is blank and the assignment is in a method (or, static, in a constructor), and a record component outside its canonical constructor. A blank final in its own constructor is left to definite assignment. |
| `package_mismatch` | `error` | the declared `package …;` doesn't match the file's location under its source root (needs `expected_package`, inferred from the path). The `change_package` helper produces the "set package to …" quick-fix edit. |
| `version_errors` | `error` | a language feature used below the project's target Java version — records (16), sealed types (17), `var` (10), text blocks (15), switch arrows (14), lambdas / method references (8), try-with-resources (7), multi-catch (7), default/private interface methods (8/9). Needs `java_major`. A `var` is *not* flagged when the file imports Lombok's `var` (back-ported below 10). |
| `ctor_check_errors` | `warning` | a `method_declaration` named exactly like its enclosing class/enum — an intended constructor written with a return type, which Java silently accepts as an ordinary method. Only class/enum; a real (return-typeless) constructor parses as a different node so never matches. (The "explicit constructor call must be first / can't be both `this()` and `super()`" cases are left to `syntax_errors` — the grammar rejects a misplaced chain call as an `ERROR`.) |
| `generics_syntax_errors` | `error` | syntactic generics misuse: generic array creation (`new List<String>[]`), instantiating a type parameter (`new T()`), generics in an `instanceof` (a concrete `List<String>`, not an unbounded `?`) or a `catch` type, and `this`/`super` in a `static` context. Scoped type-parameter set gathered from enclosing declarations; anonymous/nested-type carve-outs keep `this`/`super` sound. |
| `erasure_clash_errors` | `error` | two overloads in one type that are distinct in source but identical after generic type erasure (`f(List<String>)` vs `f(List<Integer>)`). Erases by stripping type arguments (keeps primitives/arrays); a bare single type-variable parameter is skipped (bound unknowable); byte-identical signatures are left to `duplicate_signatures`. |
| `iface_dup_errors` | `error` | the same interface listed twice in one `implements`/`extends` clause — by erased simple name, so `List<String>, List<Integer>` (different type arguments) and a plain `Foo, Foo` both flag. No resolver — purely the written list. |
| `switch_flow_warnings` | `warning` | colon-style `switch` **fall-through** (a non-empty, non-last `case` group whose last statement plainly slides into the next label) and a `return`/`break`/`continue` inside `finally` that discards a pending exception/result. Both conservative — stacked empty labels, arrow switches, and jumps targeting a construct nested in the `finally` are never flagged. |
| `expr_lint_warnings` | `warning` | self-assignment (`x = x`, `this.x = this.x`), a constant integer division/modulo by zero, a stray empty statement (`;` as a block statement), and comparing strings with `==`/`!=` (a `String` literal on either side — reference, not contents). |
| `unused_imports` | `warning` | a single-type import whose name never appears elsewhere (identifiers *or* comments). `static`/wildcard skipped. |
| `duplicate_imports` | `warning` | a repeated identical import. |
| `redundant_imports` | `warning` | a redundant **wildcard** import — `import java.lang.*;` (implicitly imported) or `import <own package>.*;` (same package already in scope). Purely syntactic (own package read off the tree), so no resolver and never a false positive; `static` wildcards and single-type imports are left alone. |
| `var_target_errors` | `error` | a `var` local whose initializer has no type of its own — a lambda, a method/constructor reference, an array initializer (`var xs = {1,2};`), or the `null` literal. Only the direct value (a cast supplies a target, so `var r = (Runnable) () -> {}` is fine); non-`var` declarations untouched. |
| `capture_errors` | `error` | a local captured by a lambda **or** an anonymous/inner class and then reassigned in its declaring method (`int c = 0; Runnable r = () -> use(c); c = 5;`) — not effectively final. Complements `lambda_capture_errors` (mutation *inside* a lambda). Conservative like the `final` check: only a local WITH an initializer, declared once, reassigned outside any closure, and actually captured — so a definite-assignment-safe local is never flagged. |

The blank-final half of the definite-assignment check (code `definite-assignment`: an instance `final`
field with no initializer that a non-delegating constructor leaves unassigned — reported on that
constructor's closing brace, as javac does — or, with no constructor or for a static field, one
assigned nowhere in its class; silent under a Lombok constructor annotation) is also exposed as
`uninitialized_final_fields(root, source)` — the field name spans — so the quick-fixes that initialise
the field (`bennu-refactor`'s `final_field_fixes`) act on exactly the fields the check flags.

Constant expressions (JLS §15.29) are folded in one place, `support::constant`: `switch_dup` compares
`case 1 + 1` with `case 2` and `case PREFIX` with `case "a"`, and `narrowing` accepts `byte b = K;`
when `K` is a constant that fits.

`check_file` takes a `FileContext { file_stem, expected_package, java_major, classpath_complete }` —
each `None` field just skips its check, so a scratch buffer still gets every source-only diagnostic.
`classpath_complete` (default `false`) tells the unresolved-import check whether the dependency jars
were resolved: when `false` it adjudicates only `java.*` imports (the JDK-authoritative namespace), so
an unindexed `javax.*` / library import is never a false "cannot resolve".

## Resolver-backed checks (`check_file_resolved`, runs type inference)

All of these run through the shared, conservative supertype walk in `walk.rs` (`hierarchy_has` /
`for_each_supertype` / `reaches` / `hierarchy_fully_known`): an unknown class in a walk is treated as
"might satisfy", so a positive "wrong" verdict is only reached over a fully-known hierarchy. They run
only when `jdk_available`.

| Check | Emits | What |
|---|---|---|
| `unresolved_imports` | `error` | a single-type `import a.b.C;` whose type the resolver can't find (a typo / removed class). `static`/wildcard skipped; nested types tried as `a/b/Outer$Inner`. Only `java.*` is checked unless `classpath_complete` — a `javax.*`/library import can't be judged missing without the dependency jars (never a false positive). |
| `unknown_members` | `error` | a call `receiver.method(...)` whose `method` doesn't exist on the receiver's **inferred** type (walking supertypes). |
| `unknown_fields` | `error` | a `receiver.field` access whose `field` doesn't exist on the inferred type. Skips array `length`, static qualifiers, package/type prefixes. |
| `arity_errors` | `error` | a `recv.method(args)` / bare `method(args)` / `new Foo(args)` whose argument count matches no overload (varargs-aware — a trailing array is treated as possibly-varargs; the count rule is `bennu-java`'s shared `arity_admits`). A bare call is judged against the single top-level type's fully-known hierarchy plus the file's own declarations, lambdas included (a lambda adds no methods), nested/anonymous classes excluded. Silent when the method is *missing* (that's `unknown_members` / `unresolved_call`). |
| `argument_type_errors` | `error` | an argument whose type can't bind to the parameter (`foo(1)` where `foo(String)`). Which overloads apply is `bennu-java`'s shared `overload_fit` (arity with varargs, strict/loose/varargs phases, lambdas vs functional interfaces, `null`, boxing, subtyping; unknown never excludes): reported only when it proves **no** overload applicable and every overload the count admits is refused on definite evidence (String/boolean↔primitive, a primitive whose box can't reach the class, a non-box class for a primitive, a reference type the argument isn't a subtype of — over a readable superclass chain for a class, a `final` class regardless, the whole hierarchy for an interface; `Object` arguments abstain). Positions whose parameter is a type variable, an array, or at/after a trailing array are not judged; the rest of a generic or varargs method is. An argument refused by every admitted overload is flagged naming each expected type, otherwise the call is. Reads value, `this`, `super` and static (`Util.m(…)`) receivers, bare calls (lambdas crossed; Lombok and static imports don't block, a member name shadows them), and `new` including anonymous bodies. A lambda, method reference, `null` or untyped argument is never reported. |
| `unresolved_types` | `error` | a simple type name in a type position (`Fooo x;`, `extends Barr`, `List<Bazz>`, `catch (Quxx e)`) the resolver can't resolve. Excludes in-scope type parameters, same-file types, `var`, `java.lang`, and **member types inherited from a supertype** (JLS §8.1.5 — `class Sub extends Base` sees `Base`'s nested `Inner` as a bare `Inner`, with nothing to import). |
| `type_arg_arity_errors` | `error` | a `Base<A, B, …>` whose type-argument count ≠ the number of type parameters `Base` declares (`List<A, B>`, `Map<String>`) — using the seam's `type_params` (bytecode generic signature for library/JDK types, the `<T, …>` clause for project types). Flags only when the base resolves AND its `type_params` is non-empty (exact arity known); the diamond `<>`, wildcards, raw types, an unresolved base and a scoped/nested-generic base are skipped, so never a false positive. |
| `type_compat_errors` | `error` | an inconvertible cast (`(String) anInteger`), and an assignment / `return` whose value's type is incompatible with the declared type — including a `String` ↔ primitive mismatch (`int x = "1";`, `int y = "1" + 1;`, `String s = 1;`), driven by literal + string-concatenation typing. Reference-to-reference is flagged only between unrelated concrete classes over a fully-known hierarchy; boxing / widening / interfaces / generics are left alone. `java/lang/Object` on either side of a cast/assignment is skipped (universal supertype; also an erased-generic value). A **chained** method call (`a.b().c()`) value is skipped: shallow generic substitution can mis-type a chain (`list.stream().map(X::getId).max(..).orElse(null)` → the element type, not the mapped result), so it's left to the compiler. |
| `visibility_errors` | `error` | a `receiver.member` (or `Type.staticMember`) reaching a member the site can't see: a `private` member accessed from outside its declaring **top-level** type (an outer class and its nested types share one nest → never flagged between them), or a package-private member from another package. Extremely conservative — only over a fully-known hierarchy, an unambiguous single declaration, `Public`/`Protected` never flagged, and **only on the project's own types** (JDK and dependency-jar members are exempt: their real accessibility — generated accessors, split packages, module rules — isn't decidable from bytecode). |
| `inheritance_errors` | `error` | an illegal `extends`/`implements`: a class extending a `final` type / record / enum / interface, a class implementing a non-interface, an interface extending a non-interface. Uses the class-level `ClassFlags` decoded from bytecode. |
| `missing_abstract_impls` | `error` | a concrete class that leaves an inherited abstract method unimplemented. Requires the whole hierarchy known; `Object` methods never count; `sealed` supertypes are not consulted. |
| `functional_errors` | `error` | a lambda whose parameter count doesn't match its target functional interface's single abstract method, or whose target interface isn't functional. Only for explicit targets (`T x = …`, `return …`, `(T) …`) against a known interface. |
| `super_constructor_errors` | `error` | a subclass constructor that doesn't chain (`super(...)`/`this(...)`) when the superclass has no no-arg constructor (or a subclass with no constructor at all). Runs only when the superclass's constructors are indexed (bytecode) — a conservative miss otherwise. |
| `final_override_errors` | `error` | a method that overrides a `final` supertype method (`final` methods can't be overridden). Matched by name **and** erased parameter types (a legal overload is never flagged), and only when every parameter type resolves. Fires against `final` methods of JDK/library supertypes (incl. `java.lang.Object`'s `final` `wait`/`getClass`/…) and project supertypes. |
| `override_return_errors` | `error` | an override whose return type isn't covariant — `String get()` overriding `Number get()`. Matched by name **and** erased parameter types (a real override, never an overload); flags only when BOTH return types are concrete reference classes over fully-known hierarchies and the overriding one is NOT a subtype of the overridden one. Generics erased to a shared bound, primitive/`void` returns, and un-indexed supertypes are skipped. Complements `inherit_cycle_errors`' `@Override`-overrides-nothing, which deliberately leaves a name match alone. |
| `inherit_cycle_errors` | `error` | cyclic inheritance — a type that transitively `extends`/`implements` itself. Flags only when every link on the path back is a resolvable type and the walk closes on the exact starting binary (never inferred through an unknown link). Plus an `@Override` (reported on the annotation) on a `static` method, in a class with no written supertype on a method `Object` does not declare, or — over a **fully-known** hierarchy — on a name that exists nowhere in it or only with other parameter counts (an overload). Varargs methods are not compared. |
| `super_method_errors` | `error` | a `super.method()` whose name exists nowhere in the enclosing class's superclass/interface hierarchy. Name-only match (overloads/generics never trigger it); requires the whole super-hierarchy known and skips qualified `Outer.super` and anonymous-class receivers. |
| `exception_errors` | `error` | an unreachable `catch` (a type `==`/subtype of a clause above), a multi-`catch` listing a type with its supertype, and a try-with-resources whose resource type definitively isn't `AutoCloseable`/`Closeable`. Subtype verdicts require the subtype's whole hierarchy known (an unknown link → skip), so an unresolved exception/resource is never flagged. |
| `enum_switch_errors` | `error` | a `switch` **expression** over an enum — or a statement made exhaustive-by-law by a `case null` — with no `default` that leaves some constant uncovered (names the missing ones). Other statement-form switches are legal and never flagged; a pattern label may be total and silences it; the enum type must fully resolve (`is_enum`) and its constants be completely enumerable, else skip. |

`ClassFlags` (interface / abstract / final / enum / record / sealed) and the per-member
`is_abstract` / `is_default` / `is_final` come from **two** sources, unified across the seam: `bennu-classpath`
decodes them from **bytecode** (JDK / library types), and `bennu-intel`'s java-index derives them from
the **source symbol model** (`TypeKind` + the `abstract`/`final`/`sealed` modifiers + bodyless
interface methods) for **project** types. So the inheritance / implement-abstract / functional checks
now fire against project supertypes too — `class X extends ProjectFinal {}`,
`class Y implements ProjectIface {}` with an unimplemented method — not only bytecode ones. The
`missing_abstract_impls` guard still requires the **whole** hierarchy known, so a project type whose
own supertype is un-indexed stays a conservative miss (never a false positive).

```rust
use bennu_check::prelude::*;

for d in check_file(source, &FileContext::default()) {
    // d.severity ("error" | "warning"), d.message, d.start..d.end (UTF-8 byte offsets)
}
```

`check_file` parses once, runs every pure-AST check, and returns the diagnostics ordered by position
(capped at `MAX_DIAGNOSTICS`); `check_file_resolved` adds the resolver-backed checks over one shared
parse. All API is re-exported from `bennu_check::prelude`.

## Incremental validation (out-of-code-block)

`check_file_resolved_incremental(source, ctx, resolver, jdk, resolver_rev, &mut IncrementalCache)`
([`incremental`]) is a drop-in for `check_file_resolved` that re-runs the **expensive per-expression**
resolver checks only over the method/constructor **body** whose text changed, replaying cached
diagnostics (rebased) for the unchanged ones — so re-validating a big class while you type inside one
method stays cheap. The cheap checks (all pure-AST + the declaration-oriented resolver checks) always
run fresh, and the expensive checks run over a true **partition** of the nodes (each top body + the
structural remainder), so the result is a byte-for-byte-equivalent multiset to a full run. A body is
reused only when the file's `structural_hash` (signatures/fields/imports), the caller's `resolver_rev`
(project re-index / another file edited), and the body's own text hash all match. The equivalence is
pinned by the `incremental` tests — a fresh run, and every run after a body/signature/field/rev/add
edit, must reproduce `check_file_resolved`. Wired live in `bennu-be`'s `validate_java` (the resolved
tier); the whole-project run stays a plain full pass.

## Diagnostic kinds (`CheckId`)

Every `Diagnostic` carries a stable `code` — a kebab-case kind slug (`"unknown-member"`,
`"wrong-type-argument-count"`, …) from the [`CheckId`] catalog (`check_id`), the way IntelliJ's
`JavaErrorKinds` names every error. A check emits `CheckId::UnknownMember.at(node, msg)` /
`CheckId::X.span(start, end, msg)` and the `code` + default `severity` come from the catalog, so a
check can't disagree with itself and the wording/severity for a kind lives in ONE place. This is what
lets the FE / settings group, suppress or re-severity a rule by kind and lets a future quick-fix
registry key off the kind instead of matching message text. Migration to the catalog is **incremental**
— diagnostics not yet moved over carry an empty `code` (still valid); the resolver-backed
member/type/arity kinds are migrated first.

## Message shape

A message quotes the code it is about, and the code it is about is whatever the parser found there —
which can be a chained expression, an array initializer, or a string literal holding a pasted
document. Text going into a message therefore goes through `text::short` (one flat line, 60 chars,
ellipsis), and every entry point returns through `check::finish`, which orders by position, caps the
count at `MAX_DIAGNOSTICS` and each message at `MAX_MESSAGE_CHARS`. The span already points at the
whole thing, so the message never needs to reproduce it — and a message that is not a sentence
becomes a tooltip the size of the file.

## Differential corpus (javac as the oracle)

`tests/corpus/` is a Maven project (JDK-only). Its error modules `java8` / `java21` hold paired
`*Bad.java` / `*Ok.java` cases; its clean modules `clean8` / `clean21` hold realistic, legal code of
~100-300 lines per file that combines features the way real code does and must compile with zero
diagnostics. The system `mvn` + JDK 21 compile it, and `refresh-expected.ps1` / `.sh` turn javac's raw
diagnostics into committed golden files, flagging every inline `// error:` marker javac disagrees with
and every javac diagnostic on a clean module. The Rust side, `bennu-intel`'s `check_corpus` test,
validates every file against the real JDK: it **fails on any false positive** (a Bennu error on a line
javac accepts, or any Bennu diagnostic, warnings included, on a clean module) and writes per-category /
per-key coverage plus a clean-modules section to `CARGO_TARGET_TMPDIR/bennu-check-corpus.md`. It lives in `bennu-intel` because the
real-JDK resolver is built from that crate's dependencies. Layout, marker syntax, the refresh
workflow, how to add a case and the remaining waves: [`tests/corpus/README.md`](tests/corpus/README.md).

```sh
cd crates/products/bennu/analysis/check/tests/corpus && bash refresh-expected.sh   # or refresh-expected.ps1
cargo test -p bennu-intel --test check_corpus -- --nocapture
```

## Roadmap

Remaining depth (see `docs/bennu-indexing-validation-analysis.md`):

- **Method-reference resolution** (`Type::method` binds to a real, compatible member) and **generic
  type-parameter arity / bound** checking (`GenericsPlayground<String>` where `<T extends Number>`, an
  explicit `<Integer>` type argument that conflicts with the actual argument, PECS `add` on a `?
  extends` list, and inference over incompatible bounds).
- **Overload-resolution ambiguity** (`combo(null, null)` matching two signatures equally, an ambiguous
  `null`/lambda across varargs overloads) — needs full applicability + most-specific ranking, so it's
  deferred rather than approximated.
- **Sealed `permits` enforcement** — a subclass not listed in a `sealed` supertype's `permits` clause
  (the inheritance checks currently skip `sealed` supertypes to avoid mis-reporting a legal subclass).
- **Missing required annotation element** (`@RequiresReview` used without its no-default `reviewer`) —
  needs the annotation type's element/default set resolved.
- A whole-classpath **package index** so a wildcard `import a.b.*;` can be flagged when the package
  genuinely doesn't exist, or a name is ambiguous across two on-demand imports (today only the
  *redundant* wildcard cases are flagged — there's no cheap package enumeration over the jimage / jars).

Deliberately **not** attempted (would risk false positives without deeper flow/inference than the
conservative walk affords): full **definite-assignment** ("variable used before assigned"; the
effectively-final check only covers the definitely-illegal initialized-then-reassigned case), **raw-type
/ unchecked** warnings, and **autoboxing** NPE hints. Errors that live only in **Lombok-generated** code
(a `@NoArgsConstructor` that can't initialize a `final` field, a `@Builder` clashing with an explicit
constructor) are validated after expansion by `bennu-intel`, not here.
