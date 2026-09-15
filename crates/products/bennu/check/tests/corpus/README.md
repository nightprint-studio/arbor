# bennu-check differential corpus

A Maven project of small Java files, compiled by the **real javac**, whose output is the oracle
`bennu-check` is measured against. The validator's contract is *zero false positives*: when unsure,
stay silent. A unit test written against a mock resolver cannot prove that; javac over real code can.

- **Negatives** (`*Ok.java`) must compile cleanly. Any Bennu **error** on them is a false positive and
  fails the test. They are the point of the corpus: legal code a naive checker would flag.
- **Positives** (`*Bad.java`) carry one javac error per marked line. A Bennu miss there is *not* a
  failure; it is scored as coverage.
- **Clean modules** (`clean8`, `clean21`) hold realistic, legal, tricky code with no markers. javac
  reports nothing on them, so **any** Bennu diagnostic there, error *or warning*, is a false positive
  and fails the test. See [Clean modules](#clean-modules).

Where Bennu currently stands against the corpus (coverage, triaged false positives): [STATUS.md](STATUS.md).

## Layout

```
tests/corpus/
  pom.xml                  aggregator: compiler settings + the `oracle` profile
  java8/  pom.xml          maven.compiler.release = 8
          src/main/java/corpus/<category>/*.java
          expected.txt     javac golden file (generated, committed)
  java21/ pom.xml          maven.compiler.release = 21: var, records, sealed, switch/patterns, text blocks
          src/main/java/corpus/<category>/*.java
          expected.txt
  clean8/  pom.xml         release 8, legal code only: must compile with zero diagnostics
           src/main/java/corpus/<category>/**/*.java
           expected.txt    header only (anything below it is a corpus bug)
  clean21/ pom.xml         release 21, legal code only
           src/main/java/corpus/<category>/**/*.java
           expected.txt
  refresh-expected.ps1     regenerate the golden files (Windows PowerShell 5.1+)
  refresh-expected.sh      the same, POSIX shell; both write byte-identical output
```

The package segment after `corpus` is the **category** the report groups by (`corpus.args` → `args`,
`corpus.imports.lib` → `imports`). A case that compiles at level 8 goes in `java8`; only cases that
need a newer language feature go in `java21`.

The Rust side lives in `crates/products/bennu/intel/tests/check_corpus.rs` (+ `check_corpus_support/`).
It needs the real-JDK resolver (classpath reader, persisted index, `IndexResolver`, project indexer).
Those are `bennu-intel` dependencies, not `bennu-check` ones.

## Markers

A trailing comment on the line javac is expected to report:

```java
api.one(1, 2); // error: compiler.err.cant.apply.symbol
switch (value) { case 1: break; } // error: compiler.err.feature.not.supported.in.source.plural compiler.err.constant.label.not.compatible
```

- `// error:` or `// warn:`, then one or more full javac keys (`compiler.err.…` / `compiler.warn.…`),
  separated by spaces or commas. List every key javac reports on that line.
- Markers are the author's **guess**. javac is the truth: the refresh script prints every disagreement.
- `*Ok.java` and support files carry no markers.

## Refreshing the golden files

Needs the system `mvn` on `PATH` and a JDK 21 on `JAVA_HOME` (nothing is bundled). From this directory:

```powershell
powershell -ExecutionPolicy Bypass -File .\refresh-expected.ps1       # or: .\refresh-expected.ps1 -SkipMaven
```
```sh
bash refresh-expected.sh                                               # or: bash refresh-expected.sh --skip-maven
```

The script runs `mvn -B -P oracle -fae clean compile`, which forks javac with:

| option | why |
|---|---|
| `-XDrawDiagnostics` | `File.java:LINE:COL: compiler.err.key: args`, machine-readable |
| `-Xmaxerrs 100000 -Xmaxwarns 100000` | never truncate at 100 |
| `-XDshould-stop.ifError=FLOW` | keep running flow analysis (definite assignment, exceptions, missing return, exhaustiveness) even when attribution errors exist; javac skips it otherwise. JDK 9+ spelling; not verified on this machine |
| `-Xlint:-options` | silence the positionless "release 8 is obsolete" warning |
| `-Xstdout target/javac-raw.txt` | javac's own transcript, independent of how the plugin re-prints it |
| `<fork>true</fork>` | `-Xstdout` only captures javac's log; in-process the plugin swallows diagnostics through a listener |
| `<failOnError>false</failOnError>`, `-fae` | the corpus fails on purpose; keep both modules going |

It then writes `<module>/expected.txt` (`relative/path/File.java:LINE:COL: compiler.err.key`, sorted,
UTF-8 without BOM). If `javac-raw.txt` is missing it falls back to `[ERROR] path:[line,col] key` lines in
the Maven log. Finally it prints every **CORPUS CASE IS WRONG** row: a marker javac did not confirm, or a
javac diagnostic without a marker. For the clean modules it prints every javac diagnostic as **CLEAN
MODULE DOES NOT COMPILE - corpus bug**, and **CLEAN MODULE NOT VERIFIED** when there is no javac
transcript or no compiled class. Exit code 1 when there is any. Fix the case or the marker, then re-run
with `-SkipMaven` / `--skip-maven`.

Raw diagnostics name the file without its directory, so **file names must be unique within a module**
(the script refuses to attribute an ambiguous name).

## Opening it in IntelliJ

Open `tests/corpus/pom.xml` as a Maven project. The `oracle` profile is off by default, so IntelliJ
compiles normally and shows the same red lines javac reports. Every `*Bad.java` is red by design.

## Running the differential test

```sh
cargo test -p bennu-intel --test check_corpus -- --nocapture
```

It indexes each module against the machine's JDK, validates every file, and compares with
`expected.txt`. It skips (and says why) when no JDK resolves or a golden file is missing.

- Matching is on **file + line**, never column. A Bennu diagnostic claims the whole line range of its
  enclosing statement, because javac and Bennu can point at different lines of one multi-line call.
- The javac-key → Bennu-check mapping is `bennu_check::javac` (`coverage(key)`), the same table the
  langtools and `javac_diff` harnesses use.
- **FAILS** on false positives: a Bennu error where javac reports no error, or any Bennu diagnostic
  (error or warning, any code) on a clean module.
- Writes a markdown report to `CARGO_TARGET_TMPDIR/bennu-check-corpus.md` (with this workspace's target
  redirect: `src-tauri/target/tmp/`). It contains: summary; false positives; clean modules (files, lines,
  false positives, and javac diagnostics that would mean the corpus is broken); mislabeled (both complain,
  but Bennu's check is not mapped to javac's key); coverage by category (`hit/mapped`); coverage by
  javac key; the list of misses; marker/golden disagreements (stale golden file or wrong marker).

## Adding a case

1. Pick the category package and module (level 8 unless the case needs 21).
2. Add the failing shape to a `*Bad.java` with a marker, and its near-identical legal twin to the
   matching `*Ok.java`. Add tricky legal shapes to `*Ok.java` too.
3. Keep javac from cascading:
   - never make a support file (`*Api`, `*Types`, `ImportLib`, …) erroneous;
   - one error **class** per file: structure, annotation, attribution and flow errors live in separate
     files, because an error of one kind can hide another kind in the same class;
   - parse errors (`implements` on an interface) go alone in their own file;
   - put a switch, a flow-checked method or a constructor on **one line** when javac reports at a closing
     brace or reports selector and label separately;
   - spaces, not tabs (javac columns expand tabs).
4. Run the refresh script, fix every CORPUS CASE IS WRONG row, commit the golden file with the case.

## Clean modules

The error modules are tiny isolated files; real code is not. `clean8` / `clean21` hold files of ~100-300
lines that **combine** features the way production code does (a lambda inside an anonymous class inside a
generic inner class calling an overload chosen by boxing), with classes referencing each other across
files and packages. javac compiles them silently, so the test treats every Bennu diagnostic on them as a
false positive, warnings included.

Rules for adding code:

1. Legal Java only, JDK-only, no markers. Level 8 code goes in `clean8`; anything needing 9+ goes in
   `clean21`. Check API availability per release (`List.of`, `Optional.isEmpty`, `String.isBlank` are
   not in 8).
2. Nothing a correct lint may flag either: no unused locals or private members, no dead stores, no real
   switch fall-through, no stray `;`, no `==` on string literals. Suppressions are not applied by the
   validator the test calls, so `@SuppressWarnings` does not help.
3. Nothing javac warns about by default: no for-removal APIs, no `null` or non-`Object[]` arrays passed
   to an `Object...` parameter, no `synchronized` on value-based classes. Raw types and unchecked calls are
   fine (javac only prints a non-located note).
4. Keep file names unique within the module, then run the refresh script: `expected.txt` must stay
   header-only. Commit it with the code.

| module | category | files | lines | covers |
|---|---|---:|---:|---|
| clean8 | `overloads` | 2 | 256 | strict/loose/varargs phases, `null`, boxing vs widening, zero varargs, `Runnable`/`Callable` and `Function`/`Consumer` selection, every method-reference form |
| clean8 | `generics` | 4 | 561 | Stream/Collectors inference (`groupingBy`, `toMap`, `reducing`, `partitioningBy`, `collectingAndThen`), `Comparable<? super T>`, intersection bounds, generic `throws`, capture helpers, self-typed builders, members substituted through 2+ levels, covariant returns, explicit type arguments |
| clean8 | `structure` | 5 | 580 | inner classes and `Outer.this`, local/anonymous classes capturing effectively-final locals, shadowing, initializers, same-named field and methods, labels, enums with constant bodies/abstract methods/constructors, default/static interface methods, `Object` methods on interfaces, `X.super.m()`, protected access across packages |
| clean8 | `flow` | 2 | 360 | finals assigned on every branch/switch arm/try, `while (true)` + `break`, labeled `break`/`continue`, stacked case labels, try-with-resources, multi-catch, precise rethrow, methods ending in `throw` or an infinite loop |
| clean8 | `exceptions` | 1 | 150 | declared, caught, wrapped, narrowed by overrides, functional interfaces with `throws`, `throws X` inferred from lambdas |
| clean8 | `legacy` | 8 | 448 | `Vector`, `Hashtable`, `Enumeration`, raw collections with casts, anonymous raw `Comparator`s, `StringBuffer`, JDBC interfaces, DAO/service layering, snake_case, static single and on-demand imports, `java.util.*` + `java.awt.*` disambiguated by a single-type import |
| clean8 | `service` | 2 | 156 | the reported `Optional<Delegate>` service: presence guard, `get()` chains, the leading-comma three-argument call |
| clean21 | `records` | 2 | 212 | compact canonical and delegating constructors, static factories, generic records, records implementing interfaces, a record nested in a record, local records/enums/interfaces |
| clean21 | `sealed` | 6 | 248 | sealed interface and abstract sealed class, final/sealed/non-sealed subclasses across files, exhaustive pattern switches without `default`, nested record patterns, guards |
| clean21 | `modern` | 3 | 401 | switch expressions (arrows, `yield`, colon groups), qualified enum labels, `case null`, pattern scope out of negated `if`/`while`, `var` and `var` lambda parameters, text blocks, private interface methods, effectively-final resources, anonymous diamond, `Stream.toList()` |
| clean21 | `generics` | 1 | 83 | generic records, generic sealed interface matched exhaustively, bounds with `var` |
| clean21 | `overloads` | 1 | 106 | overloads with `var` lambda arity, record constructor references, `Interface.super::m` |
| clean21 | `structure` | 1 | 135 | generic inner class with anonymous diamond and lambda, inner-class diamond creation, local class capturing `var`s, enum with abstract methods in a record |
| clean21 | `service` | 2 | 123 | the reported service with `var`, `isEmpty()`, record patterns on the result |

## Coverage so far

Marked error lines per category (each `*Bad.java` has a clean `*Ok.java` twin):

| module | category | error lines | covers |
|---|---|---:|---|
| java8 | `resolve` | 46 | unresolved variable / method / type / supertype / import (`cant.resolve*`, `doesnt.exist`, `cant.deref`) |
| java8 | `args` | 41 | arity, argument types, conversions, constructors, anonymous classes, `super(…)`/`this(…)` |
| java8 | `overload` | 10 | strict/loose/varargs phases, boxing, widening, `null`, `ref.ambiguous` |
| java8 | `lambda` | 16 | lambda arity/return, overload selection, method references in calls and assignments |
| java8 | `mref` | 11 | unknown, arity, static-vs-instance, constructor refs, ambiguous refs |
| java8 | `generic` | 66 | generic arguments, arity, bounds, assignment, explicit type args, static type-var use, reification, wildcards, raw types (negatives), unresolved type params |
| java8 | `scope` | 15 | inherited / default / static members, static imports, nested and inner types, shadowing |
| java8 | `imports` | 30 | single static, static on-demand, on-demand, nested type imports; ambiguity (project and JDK) and its shadowing negatives |
| java8 | `annotation` | 36 | `@Target`, unknown/missing/duplicate elements, non-constant and ill-typed values, illegal member types, header clauses |
| java8 | `inherit` | 62 | final, class/interface kind, unimplemented abstract, weaker access, override rules, duplicates/erasure, super constructors, modifiers |
| java8 | `flow` | 46 | effectively-final captures, final assignment, definite assignment, missing return, unreported checked exceptions |
| java8 | `condition` | 9 | non-boolean conditions and logical operands |
| java8 | `cast` | 34 | inconvertible casts and assignments, lossy conversions, boxing mismatches |
| java8 | `switches` | 15 | selector types, duplicate/non-constant/qualified/ill-typed labels |
| java8 | `reported` | 2 | the reported `Optional<Delegate>` wrong-third-argument call, one-line and as formatted |
| java21 | `modern` | 15 | records, `var`, patterns, switch expressions, text blocks in calls |
| java21 | `inherit` | 7 | sealed hierarchies, extending a record, record modifiers |
| java21 | `records` | 12 | instance fields/initializers, accessors, canonical/compact/non-canonical constructors |
| java21 | `flow` | 4 | captures through `var`, pattern variables, switch arms |
| java21 | `switches` | 27 | selectors and constant labels, exhaustiveness, `yield`/`break`/`return`, dominance, duplicates |
| java21 | `reported` | 1 | the reported call with records and `var` |

## Waves

- [x] **Wave 1**: resolution, arguments, overloads, lambdas, generics in calls, scope, the reported case
- [x] **Assignment / casts**: incompatible assignments, casts, lossy conversion, boxing (`cast`, `generic`)
- [x] **Structure**: final/abstract/override/access-on-override/duplicates/modifiers (`inherit`), annotations
- [x] **Flow**: definite assignment, finals, captures, missing return, checked exceptions
- [x] **Level 21**: records, sealed, switch expressions and patterns, exhaustiveness
- [ ] **Access and static context**: private/protected/package access (`report.access`, `not.def.public*`),
      instance members from static contexts (`non-static.cant.be.ref`), `this`/`super` in static context,
      static interface method called through an instance
- [ ] **Instantiation**: `new` on abstract classes/interfaces (`abstract.cant.be.instantiated`), enums
- [ ] **Reachability and statements**: unreachable code, `break`/`continue` outside loops and unknown
      labels, `return` value from `void` / missing value, not-a-statement, dead `catch`
      (`except.never.thrown.in.try`), multi-catch of related types, non-`AutoCloseable` resources
- [ ] **instanceof**: inconvertible `instanceof` operands, pattern variables at level 8
- [ ] **Version-gated features** in `java8`: `var`, records, text blocks, switch expressions, private
      interface methods (`feature.not.supported.in.source*`)
- [ ] **Cross-file hierarchy**: overrides, erasure clashes and abstract methods inherited across files
      and generic supertypes; interface/abstract-class diamonds
- [ ] **Enums**: constructors, constant bodies, `switch` on enums from another package
- [ ] **Lombok and dependencies**: needs a corpus module with Maven dependencies
