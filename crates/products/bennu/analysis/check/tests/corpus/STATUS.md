# bennu-check vs javac — current status

What the corpus expects, and where bennu-check stands against it.

- **What is expected** is `java8/expected.txt` and `java21/expected.txt`: every error javac reports,
  one `file:line:col: key` per line. A `*Bad.java` file is expected to produce exactly those errors; a
  `*Ok.java` file is expected to produce **none**. Inline `// error:` markers repeat the same
  expectation next to the code, and `refresh-expected` checks the markers agree with javac.
- **Where Bennu stands** comes from `cargo test -p bennu-intel --test check_corpus -- --nocapture`,
  which writes the per-line report to `src-tauri/target/tmp/bennu-check-corpus.md` (every miss, every
  false positive, coverage per javac key). This page is a snapshot of that report, triaged by hand.

Snapshot: JDK 21.0.6, `release 8` and `release 21`.

## Summary

Error modules — javac errors Bennu should report:

| module | files | javac errors | mapped to a Bennu check | Bennu hits | not covered (no check yet) | false positives |
|---|---:|---:|---:|---:|---:|---:|
| java8 | 123 | 445 | 427 | 235 (55%) | 18 | 2 |
| java21 | 24 | 70 | 67 | 54 (81%) | 3 | 4 |
| **total** | **147** | **515** | **494** | **289 (59%)** | **21** | **6** |

Clean modules — realistic legal code; javac reports nothing, so every Bennu diagnostic is a false positive:

| module | files | lines | javac diagnostics | Bennu false positives |
|---|---:|---:|---:|---:|
| clean8 | 24 | 2511 | 0 | 0 |
| clean21 | 16 | 1308 | 0 | 0 |

## Remaining false positives (6) — all parser gaps

tree-sitter-java 0.23 does not parse these legal (or differently illegal) shapes, so Bennu reports a
syntax error where javac reports something else, or nothing:

| where | Bennu says | the Java the parser rejects |
|---|---|---|
| `modern/ModernOk.java:52`, `modern/ModernBad.java:51` | `syntax-error` + `missing-token` | a **qualified** record pattern, `value instanceof ModernTypes.Point(int x, int y)` — the grammar accepts only a simple or generic type name before the component list |
| `annotation/AnnotationDeclBad.java:29,44` | `missing-token` / `syntax-error` | malformed `@interface` members (`<T> String value();`); javac reports a semantic error one line later |

Fixing them means a grammar update (a dependency change), not a check.

## Fixed in this round

| was | now |
|---|---|
| C1 `records/Money.java` — the implicit canonical constructor ignored beside an extra constructor | the synthetic canonical constructor is suppressed only by one taking the component types; a compact constructor carries the components as parameters |
| C2 `modern/ModernIdioms.java` — an interface with `private` methods "not functional" | `private` interface methods are never abstract requirements |
| C3 `modern/SwitchExpressions.java` — colon groups falling through to a `yield` | only the last group of a switch expression must not complete normally |
| C4 `overloads/ModernOverloads.java` — `Greeter.super::greet` | `super` / `this` qualifiers are not types |
| methods of an anonymous class, `outer.new Inner()` | receivers whose type no name denotes are not judged |
| implicit narrowing of constants (`char c = 'a' + 1`, `byte b = CONSTANT`) | constant expressions are folded (`support::constant`) |
| `import static pkg.Outer.Nested;` | member types count as statically importable members |
| duplicate / erasure clash on the first declaration; blank final on the field; abstract method on the method; `@Override` on the method name | reported where javac reports it: the later declaration, the constructor's closing brace, the class header, the annotation |

## Coverage by category (hits / mapped javac errors)

| module | category | hits | biggest gaps |
|---|---|---:|---|
| java8 | annotation | 9/35 | project `@Target` applicability (5), annotation member types (4), non-constant values (4), value types (6) |
| java8 | args | 22/40 | constructor arguments (8), argument types in several shapes (9) |
| java8 | cast | 22/34 | boxing mismatches (6), inconvertible casts (5) |
| java8 | condition | 5/7 | non-boolean conditions in less common positions |
| java8 | flow | 29/45 | unreported checked exceptions (7), definite assignment of locals (4), `Math.PI = 3` / `array.length = 3` |
| java8 | generic | 20/60 | generic assignment `List<Object> = List<String>` (9), generic method arguments (12), wildcards (5), reifiable-type rules (7), explicit type args (5) |
| java8 | imports | 17/30 | `ref.ambiguous` between two on-demand imports (7), argument types through static imports (5) |
| java8 | inherit | 49/61 | anonymous classes missing an abstract method (3), overload instead of override (1), override rules through the resolver (3) |
| java8 | lambda | 4/16 | lambda arity / return type against the target overload (12) |
| java8 | mref | 2/11 | incompatible method references (9) |
| java8 | overload | 1/10 | ambiguous calls `ref.ambiguous` (7), no applicable overload (2) |
| java8 | resolve | 32/46 | variables in unusual scopes (7), unresolved on-demand/static imports |
| java8 | scope | 10/15 | arguments across nested/inner scopes (4) |
| java8 | switches | 11/15 | qualified enum label below Java 21, `Object` selector below Java 21 |
| java21 | flow | 3/4 | captured non-final locals |
| java21 | inherit | 8/8 | — |
| java21 | modern | 9/15 | record-pattern arguments, pattern-variable scope |
| java21 | records | 14/14 | — |
| java21 | switches | 19/25 | sealed and pattern exhaustiveness (3), arm types (2) |

Keys with **no** Bennu check yet ("not covered", 21 occurrences): `not.within.bounds` (7),
`pattern.dominated` (2), `override.static`, `static.imp.only.classes.and.interfaces`,
`incompatible.thrown.types.in.mref`, `anon.class.impl.intf.no.args`, `array.and.varargs`,
`const.expr.req`, `string.const.req`, `operator.cant.be.applied`, `operator.cant.be.applied.1`,
`enum.annotation.must.be.enum.constant`, `flows.through.to.pattern`, `cant.extend.intf.annotation`.

## How to refresh this page

1. `refresh-expected.ps1` (or `.sh`) — regenerates the golden files; it must end with "Every marker agrees with javac and every clean module compiles silently".
2. `cargo test -p bennu-intel --test check_corpus -- --nocapture` — regenerates the report.
3. Update the tables above from the report's Summary, False positives and Coverage sections.
