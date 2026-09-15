# bennu-check vs javac — current status

What the corpus expects, and where bennu-check stands against it.

- **What is expected** is `java8/expected.txt` and `java21/expected.txt`: every error javac reports,
  one `file:line:col: key` per line. A `*Bad.java` file is expected to produce exactly those errors; a
  `*Ok.java` file is expected to produce **none**. Inline `// error:` markers repeat the same
  expectation next to the code, and `refresh-expected` checks the markers agree with javac.
- **Where Bennu stands** comes from `cargo test -p bennu-intel --test check_corpus -- --nocapture`,
  which writes the per-line report to `src-tauri/target/tmp/bennu-check-corpus.md` (every miss, every
  false positive, coverage per javac key). This page is a snapshot of that report, triaged by hand.

Snapshot: JDK 21.0.11, `release 8` and `release 21`.

## Summary

Error modules — javac errors Bennu should report:

| module | files | javac errors | mapped to a Bennu check | Bennu hits | not covered (no check yet) | false positives |
|---|---:|---:|---:|---:|---:|---:|
| java8 | 123 | 445 | 428 | 398 (93%) | 17 | 2 |
| java21 | 24 | 70 | 67 | 62 (93%) | 3 | 4 |
| **total** | **147** | **515** | **495** | **460 (93%)** | **20** | **6** |

Clean modules — realistic legal code; javac reports nothing, so every Bennu diagnostic is a false positive:

| module | files | lines | javac diagnostics | Bennu false positives |
|---|---:|---:|---:|---:|
| clean8 | 24 | 2511 | 0 | 0 |
| clean21 | 16 | 1308 | 0 | 0 |

> [!IMPORTANT]
> **The clean modules are the only false-positive measure of this round**
> No real project (commons-lang, guava, a Lombok codebase) was re-checked. The rules below were written
> to stay silent on anything they cannot prove, but a run on real code is what confirms it.

## Remaining false positives (6) — all parser gaps

tree-sitter-java 0.23 does not parse these legal (or differently illegal) shapes, so Bennu reports a
syntax error where javac reports something else, or nothing:

| where | Bennu says | the Java the parser rejects |
|---|---|---|
| `modern/ModernOk.java:52`, `modern/ModernBad.java:51` | `syntax-error` + `missing-token` | a **qualified** record pattern, `value instanceof ModernTypes.Point(int x, int y)` — the grammar accepts only a simple or generic type name before the component list |
| `annotation/AnnotationDeclBad.java:29,44` | `missing-token` / `syntax-error` | malformed `@interface` members (`<T> String value();`); javac reports a semantic error one line later |

Fixing them means a grammar update (a dependency change), not a check.

## Fixed in this round

java8 went from 278 to 398 hits and java21 from 54 to 62, with no new false positive and the clean
modules still silent.

| was | now |
|---|---|
| a local read before every path assigned it, or a `final` local assigned twice, unreported | JLS chapter 16 definite (un)assignment, loops, `try`, `switch` and labels included (`flow/definite.rs`) |
| checked exceptions in lambdas, initializers and anonymous-class methods, rethrows and implicit `close()` unjudged | one handler model per site (`throwing/checked_throw.rs`), precise rethrow and the resource's `close()` (`throwing/checked_call.rs`) |
| `Math.PI = 3`, `values.length = 3` | assignment to a final field through a qualifier (`flow/qualified_finals.rs`) |
| two on-demand imports offering one name; a missing package; a static import of nothing | `source/import_ambiguity.rs`, `source/imports.rs` |
| `@Target`, annotation value types and non-constant values | `annotation/annotation_target.rs`, `annotation_values.rs`, `annotation_constants.rs` |
| a static method hiding an instance one, an incompatible or `void` override, an abstract method implemented by an overload | signature-based obligations (`hierarchy/obligations.rs`), anonymous bodies included |
| bare calls in nested and anonymous classes, static-import calls, receiver type arguments | JLS §15.12.1 lookup (`support/bare_call.rs`); `Box<String>.set(1)` judged through the argument (`calls/arguments.rs`) |
| a primitive dereferenced, `Type.FIELD` on a readable type, an unknown qualifier or package | `calls/fields.rs`, `calls/members.rs`, `typing/types.rs` |
| lambda bodies and method references against their interface | void/value compatibility, return types, static/bound/unbound forms and their ambiguity, constructor references, a class as target (`calls/lambda_body.rs`) — in declarations, returns, casts and bound calls |
| `(int) true`, `(long[]) ints`, a final class cast to an interface it does not implement | `typing/casts.rs`, whose reference-cast rule (`provably_inconvertible`) pattern labels share |
| a qualified enum label below Java 21; a switch on `Object`, its constant labels and its exhaustiveness; a sealed switch missing a permitted subtype; `case Integer i` on a `String`; a switch-expression arm of the wrong type | `switching/switch_label_type.rs`, `switching/enum_switch.rs`, `typing/casts.rs` |
| `List<Integer>` into a `List<Number>`, `List<?>` into a `List<String>` | invariance and wildcard containment where both sides write their arguments (`typing/parameterized.rs`) |
| `new T[10]`, `T.class`, `List<int>`, a generic `Throwable`, `String<Integer>` | `source/generics_syntax.rs`, `hierarchy/inheritance.rs`, `typing/type_arg_arity.rs` |

## Coverage by category (hits / mapped javac errors)

| module | category | hits | biggest gaps |
|---|---|---:|---|
| java8 | annotation | 26/35 | `AnnotationDeclBad` is quarantined whole by parser recovery (9) |
| java8 | args | 38/40 | `outer.new Inner(…)` arguments (1), a conditional whose arms need a least upper bound (1) |
| java8 | cast | 34/34 | — |
| java8 | condition | 7/7 | — |
| java8 | flow | 45/45 | — |
| java8 | generic | 42/60 | calls through wildcards, bounds and raw receivers (11), explicit type arguments (3), a generic method's result assigned (2), `? super` read as its bound (1) |
| java8 | imports | 30/30 | — |
| java8 | inherit | 62/62 | — |
| java8 | lambda | 15/16 | an implicit lambda that fits two overloads (1) |
| java8 | mref | 11/11 | — |
| java8 | overload | 10/10 | — |
| java8 | resolve | 46/46 | — |
| java8 | scope | 15/15 | — |
| java8 | switches | 15/15 | — |
| java21 | flow | 4/4 | — |
| java21 | inherit | 8/8 | — |
| java21 | modern | 10/15 | a pattern variable used where a negated `instanceof` leaves it unbound (1), a qualified record pattern (parser, 1), a switch expression, a `var` element and a `toList()` result as arguments (3) |
| java21 | records | 14/14 | — |
| java21 | switches | 25/25 | — |

Keys with **no** Bennu check yet ("not covered", 20 occurrences): `not.within.bounds` (7),
`pattern.dominated` (2), `static.imp.only.classes.and.interfaces`, `incompatible.thrown.types.in.mref`,
`anon.class.impl.intf.no.args`, `array.and.varargs`, `const.expr.req`, `string.const.req`,
`operator.cant.be.applied`, `operator.cant.be.applied.1`, `enum.annotation.must.be.enum.constant`,
`flows.through.to.pattern`, `cant.extend.intf.annotation`.

## How to refresh this page

1. `refresh-expected.ps1` (or `.sh`) — regenerates the golden files; it must end with "Every marker agrees with javac and every clean module compiles silently".
2. `cargo test -p bennu-intel --test check_corpus -- --nocapture` — regenerates the report.
3. Update the tables above from the report's Summary, False positives and Coverage sections.
