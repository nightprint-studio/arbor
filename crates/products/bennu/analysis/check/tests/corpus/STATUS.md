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
| java8 | 123 | 445 | 427 | 199 (47%) | 18 | 18 |
| java21 | 24 | 70 | 61 | 26 (43%) | 9 | 5 |
| **total** | **147** | **515** | **488** | **225 (46%)** | **27** | **23** |

Clean modules — realistic legal code; javac reports nothing, so every Bennu diagnostic is a false positive:

| module | files | lines | javac diagnostics | Bennu false positives |
|---|---:|---:|---:|---:|
| clean8 | 24 | 2511 | 0 | 0 |
| clean21 | 16 | 1308 | 0 | 11 |

## Clean-module false positives (11) — all real Bennu bugs

| # | where | Bennu says | the legal Java it rejects |
|---|---|---|---|
| C1 | `records/Money.java:24,28,37,41` | `argument-type` against `Money(long, String)` | a record with an explicit extra constructor: the **implicit canonical constructor** `Money(BigDecimal, Currency)` (here declared in compact form) is not considered |
| C2 | `modern/ModernIdioms.java:137` | `lambda-arity`: `Formatter` is not a functional interface | an interface with one abstract method plus `default` and **`private` / `private static`** methods (Java 9) |
| C3 | `modern/SwitchExpressions.java:78` | `switch-expression-incomplete` | a switch **expression** with old-style `case X:` groups that fall through to a `yield` |
| C4 | `overloads/ModernOverloads.java:98` | `unresolved-type`: `super` | the method reference `Greeter.super::greet` |

## Error-module false positives (23)

### Real Bennu bugs (legal code flagged)

| # | where | Bennu says | the legal Java it rejects |
|---|---|---|---|
| 1 | `args/ArgsCtorOk.java:92`, `modern/ModernOk.java:132` | `unknown-member` … in `Object` | a method declared in an anonymous class, called on the creation expression (`new Object() { int extra() … }.extra()`) or through a `var` holding it |
| 2 | `cast/CastLossyOk.java:14,18,20,40` | `lossy-conversion` | implicit narrowing of **constant expressions** (JLS 5.2): `char c = 'a' + 1`, `byte b = STATIC_FINAL_INT`, `byte b = finalLocalConstant`, `byte b = 'a'` |
| 3 | `imports/ImportSingleStaticOk.java:3`, `imports/ImportAmbiguousOk.java:8` | `unresolved-import` | `import static pkg.Outer.Nested;` — a static import of a **member type** |
| 4 | `scope/ScopeOk.java:97` | `unresolved-type` + `wrong-argument-count` + `unknown-member` | qualified inner-class creation `outer.new Inner()` |
| 5 | `modern/ModernOk.java:52` (and `ModernBad.java:51`) | `syntax-error` / `missing-token` | record pattern in `instanceof`: `value instanceof Point(int x, int y)` — the parser does not accept it |

### Same error, different line (javac and Bennu agree the code is wrong)

| # | where | Bennu marks | javac marks |
|---|---|---|---|
| 6 | `inherit/InheritDuplicateBad.java:9,17,26,34` | the **first** of the duplicate declarations | the **second** one |
| 7 | `flow/FlowAssignBad.java:15` | the blank `final` field | the constructor that misses it (closing brace) |
| 8 | `inherit/InheritAbstractBad.java:26` | the `abstract` method | the non-abstract class header |
| 9 | `annotation/AnnotationDeclBad.java:29,44` | a syntax error (the parser rejects `<T> String value();` in an `@interface`) | a semantic error on line 30 |

Fixing 6-8 means reporting on javac's line (IntelliJ marks both sides of a duplicate); 9 is a parser gap.

## Coverage by category (hits / mapped javac errors)

| module | category | hits | biggest gaps |
|---|---|---:|---|
| java8 | resolve | 32/46 | variables in unusual scopes (7), `super.`/`this.` calls, unresolved on-demand/static imports |
| java8 | imports | 17/30 | `ref.ambiguous` between two on-demand imports (7), argument types through static imports (5) |
| java8 | args | 23/40 | constructor arguments (8), argument types in several shapes (9) |
| java8 | overload | 1/10 | ambiguous calls `ref.ambiguous` (7), no applicable overload (2) |
| java8 | lambda | 4/16 | lambda arity / return type against the target overload (12) |
| java8 | mref | 2/11 | incompatible method references (9) |
| java8 | generic | 15/60 | generic assignment `List<Object> = List<String>` (9), generic method arguments (12), wildcards (5), static context (5), reifiable-type rules (7), explicit type args (5) |
| java8 | cast | 22/34 | boxing mismatches (6), inconvertible casts (5) |
| java8 | condition | 5/7 | non-boolean conditions in less common positions |
| java8 | flow | 17/45 | final re-assignment (8), unreported checked exceptions (7), definite assignment (6), missing return (4), captured non-final locals (2) |
| java8 | annotation | 9/35 | annotation member types (4), `@Target` applicability (5), non-constant values (4), value types (6) |
| java8 | inherit | 33/61 | illegal modifiers (8), `@Override` / override rules (6), unimplemented abstract methods (6), duplicates (4, line mismatch above), constructors without the super constructor (2) |
| java8 | switches | 6/15 | duplicate labels, qualified enum labels, label types |
| java8 | scope | 11/15 | arguments across nested/inner scopes (4) |
| java21 | records | 1/13 | canonical/compact constructor rules (7), accessors (2), instance initializers |
| java21 | switches | 9/23 | exhaustiveness (5), switch-expression body rules (7) |
| java21 | modern | 9/15 | pattern-variable scope, record-pattern arguments |
| java21 | inherit | 3/5 | sealed hierarchies |
| java21 | flow | 3/4 | captured non-final locals |

Keys with **no** Bennu check yet ("not covered", 27 occurrences) include `not.within.bounds` (7),
`pattern.dominated`, `override.static`, the sealed-class keys, `static.imp.only.classes.and.interfaces`,
`incompatible.thrown.types.in.mref`, `first.statement.must.be.call.to.another.constructor`,
`anon.class.impl.intf.no.args`, `array.and.varargs`, `const.expr.req`, `string.const.req`,
`operator.cant.be.applied`.

## How to refresh this page

1. `refresh-expected.ps1` (or `.sh`) — regenerates the golden files; it must end with "Every marker agrees with javac and every clean module compiles silently".
2. `cargo test -p bennu-intel --test check_corpus -- --nocapture` — regenerates the report.
3. Update the tables above from the report's Summary, False positives and Coverage sections.
