# bennu-check

Java **validation without compiling** — the "red squiggle before you run Maven". Two tiers: pure
tree-sitter-java scans (`check_file`, no resolver) and resolver-backed checks that run type
inference (`check_file_resolved`). Both emit the wire `Diagnostic` (byte offsets) the Problems panel
+ lint gutter already render. Leaf-ish crate (depends only on `bennu-java` + `bennu-proto`) —
exhaustively unit-tested here, including with real type inference.

## Pure-AST checks (`check_file`, no resolver)

| Check | Emits | What |
|---|---|---|
| `syntax_errors` | `error` | tree-sitter `ERROR` / `MISSING` nodes (missing `;`/`)`, unclosed brace). Multi-line spans clamped to the first line; clean subtrees pruned. |
| `invalid_statements` | `error` | an expression JLS §14.8 forbids as a statement — a bare field access (`stepper.add;`), a lone identifier, arithmetic (`1+1;`). Catches the classic forgotten call (`list.clear;`). |
| `missing_return` | `error` | a non-`void` method whose body can complete without a `return`/`throw`. Conservative (loops/`switch`/`try` assumed to return) → never false-flags. |
| `declaration_errors` | `error` | modifier/declaration legality: `abstract` method in a concrete class, `abstract` method with a body, `default` method outside an interface, `abstract`+`final` class, `abstract`+`private`/`static`/`final`, two visibility modifiers, `final`+`volatile` field, a `record` that's `abstract` or declares instance fields, an `enum` constant with args but no constructor. |
| `annotation_errors` | `error` | a built-in annotation on the wrong target — `@Override` off a method, `@FunctionalInterface` off an interface, `@SafeVarargs` off a method/constructor. Unknown annotations are skipped. |
| `lambda_capture_errors` | `error` | assigning / `++`/`--` a captured local inside a lambda (must be effectively final). Only enclosing-method locals are flagged, so fields are never mis-reported. |
| `class_name_matches_file` | `error` | a `public` top-level type whose name ≠ the file base name (needs `file_stem`). |
| `special_file_errors` | `error` | the restricted grammar of `package-info.java` (only a package declaration + its annotations) and `module-info.java` (only a module declaration). Keyed off `file_stem`. |
| `return_statement_errors` | `error` | a value returned from a `void` method / constructor, or a bare `return;` in a non-`void` method. Attributed to the nearest method — a `return` inside a lambda is judged against the lambda. |
| `duplicate_signatures` | `error` | two methods or two constructors in the same type with the same name and parameter types. Exact text match (generics kept) so a legal overload is never flagged. |
| `redeclaration_errors` | `error` | the same name declared twice where Java forbids it — two fields in a type, two parameters of a method/constructor/lambda, two locals in one block, or two types with the same name in one scope. Exact-name, same-scope, so a legal same-name in a disjoint sibling scope (two `for` loops each declaring `i`) or a local shadowing a field is never flagged. |
| `unreachable_code` | `error` | a statement directly after an unconditional `return`/`throw`/`break`/`continue` in the same block. Conservative — a terminator nested in an `if` doesn't kill the following code; only the first unreachable statement per block is reported. |
| `switch_yield_errors` | `error` | a `switch` **expression** arm (a block or colon group) that can complete without producing a value (`yield`/`throw`). Only in value positions. |
| `switch_selector_errors` | `error` | a `switch` on a `long` / `float` / `double` / `boolean` — types `switch` doesn't accept. Purely syntactic (declared type / literal), so no resolver needed. |
| `final_reassignment_errors` | `error` | reassigning a `final` local or field that **already has an initializer** (`final int x = 1; x = 2;`, `this.f = …` on a `final` field with an initializer). Conservative — a `final` *without* an initializer (assigned once later, possibly across `if`/`else`) is never flagged; a shadowed local name is skipped; only unambiguous `this.field` field targets are considered. |
| `package_mismatch` | `error` | the declared `package …;` doesn't match the file's location under its source root (needs `expected_package`, inferred from the path). The `change_package` helper produces the "set package to …" quick-fix edit. |
| `version_errors` | `error` | a language feature used below the project's target Java version — records (16), sealed types (17), `var` (10), text blocks (15), switch arrows (14), lambdas / method references (8), try-with-resources (7), multi-catch (7), default/private interface methods (8/9). Needs `java_major`. A `var` is *not* flagged when the file imports Lombok's `var`/`val` (back-ported below 10). |
| `unused_imports` | `warning` | a single-type import whose name never appears elsewhere (identifiers *or* comments). `static`/wildcard skipped. |
| `duplicate_imports` | `warning` | a repeated identical import. |
| `redundant_imports` | `warning` | a redundant **wildcard** import — `import java.lang.*;` (implicitly imported) or `import <own package>.*;` (same package already in scope). Purely syntactic (own package read off the tree), so no resolver and never a false positive; `static` wildcards and single-type imports are left alone. |

`check_file` takes a `FileContext { file_stem, expected_package, java_major }` — each `None` field
just skips its check, so a scratch buffer still gets every source-only diagnostic.

## Resolver-backed checks (`check_file_resolved`, runs type inference)

All of these run through the shared, conservative supertype walk in `walk.rs` (`hierarchy_has` /
`for_each_supertype` / `reaches` / `hierarchy_fully_known`): an unknown class in a walk is treated as
"might satisfy", so a positive "wrong" verdict is only reached over a fully-known hierarchy. They run
only when `jdk_available`.

| Check | Emits | What |
|---|---|---|
| `unresolved_imports` | `error` | a single-type `import a.b.C;` whose type the resolver can't find (a typo / removed class). `static`/wildcard skipped; nested types tried as `a/b/Outer$Inner`. Depends on a complete classpath. |
| `unknown_members` | `error` | a call `receiver.method(...)` whose `method` doesn't exist on the receiver's **inferred** type (walking supertypes). |
| `unknown_fields` | `error` | a `receiver.field` access whose `field` doesn't exist on the inferred type. Skips array `length`, static qualifiers, package/type prefixes. |
| `arity_errors` | `error` | a `recv.method(args)` / `new Foo(args)` whose argument count matches no overload (varargs-aware — a trailing array is treated as possibly-varargs). Silent when the method is *missing* (that's `unknown_members`). |
| `argument_type_errors` | `error` | an argument whose type can't bind to the parameter (`foo(1)` where `foo(String)`). Only when exactly one non-varargs, non-generic overload matches by arity; flags a definite mismatch only (String↔primitive, or unrelated concrete classes). |
| `unresolved_types` | `error` | a simple type name in a type position (`Fooo x;`, `extends Barr`, `List<Bazz>`, `catch (Quxx e)`) the resolver can't resolve. Excludes in-scope type parameters, same-file types, `var`, and `java.lang`. |
| `type_compat_errors` | `error` | an inconvertible cast (`(String) anInteger`), and an assignment / `return` whose value's type is incompatible with the declared type — including a `String` ↔ primitive mismatch (`int x = "1";`, `int y = "1" + 1;`, `String s = 1;`), driven by literal + string-concatenation typing. Reference-to-reference is flagged only between unrelated concrete classes over a fully-known hierarchy; boxing / widening / interfaces / generics are left alone. |
| `inheritance_errors` | `error` | an illegal `extends`/`implements`: a class extending a `final` type / record / enum / interface, a class implementing a non-interface, an interface extending a non-interface. Uses the class-level `ClassFlags` decoded from bytecode. |
| `missing_abstract_impls` | `error` | a concrete class that leaves an inherited abstract method unimplemented. Requires the whole hierarchy known; `Object` methods never count; `sealed` supertypes are not consulted. |
| `functional_errors` | `error` | a lambda whose parameter count doesn't match its target functional interface's single abstract method, or whose target interface isn't functional. Only for explicit targets (`T x = …`, `return …`, `(T) …`) against a known interface. |
| `super_constructor_errors` | `error` | a subclass constructor that doesn't chain (`super(...)`/`this(...)`) when the superclass has no no-arg constructor (or a subclass with no constructor at all). Runs only when the superclass's constructors are indexed (bytecode) — a conservative miss otherwise. |
| `final_override_errors` | `error` | a method that overrides a `final` supertype method (`final` methods can't be overridden). Matched by name **and** erased parameter types (a legal overload is never flagged), and only when every parameter type resolves. Fires against `final` methods of JDK/library supertypes (incl. `java.lang.Object`'s `final` `wait`/`getClass`/…) and project supertypes. |

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

## Roadmap

Remaining depth (see `docs/bennu-indexing-validation-analysis.md`): method-reference resolution;
generic type-parameter arity / bound checking; and a whole-classpath
**package index** so a wildcard `import a.b.*;` can be flagged when the package genuinely doesn't
exist (today only the *redundant* wildcard cases are flagged — there's no cheap package enumeration
over the jimage / jars).
