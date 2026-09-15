# bennu-test

Unit-test support for Bennu, as three pure problems. No process is spawned here and no file
is watched — that lives in `bennu-be`'s `tests` domain, where it can be cancelled.

| Module | Answers |
|---|---|
| `discover` | Which classes and methods are tests (JUnit 3/4/5, TestNG), and where they are |
| `selector` | The `mvn` argument list for a chosen scope |
| `surefire` | What happened, from `target/surefire-reports/TEST-*.xml` |
| `console` | The two output lines worth reading while the run streams |

## Three decisions worth knowing

**Discovery reads annotations, not file names.** `*Test` is a Surefire scanning default, not
a definition. On a legacy tree it is wrong in both directions: `OrderTestUtils` is a helper,
and a JUnit 3 class extending `TestCase` may be named anything at all. So a test is `@Test`
(and its JUnit 5 and TestNG relatives), or `public void testXxx()` inside a `TestCase`, or a
public method under a class-level TestNG `@Test`. Which framework decides which rule applies
comes from the file's imports — `@Test` alone proves nothing, since three frameworks and any
number of projects declare one.

**Selection uses simple class names.** Surefire's `-Dtest` dialect has changed across
versions, and 2.x — which a legacy project still pins — silently ignores fully-qualified
names and `**` globs rather than rejecting them. Combined with the `failIfNoTests=false` a
multi-module run requires, an unrecognised expression produces a green build that ran
nothing. Simple names, comma-separated, with `Class#method` for a single case, are what every
version has understood. Two identically-named classes in different packages both running is a
visible over-run; running nothing is an invisible one.

A package, a folder and a multi-selection are therefore all the same thing — a list of class
names — because there is no portable spelling for "this package and below". When that list is
too long for a command line, `plan` **widens** to the whole module or project and says so.
It never truncates: running some of what was asked for while reporting completion is the same
lie in a smaller size.

**Reports are the only per-case truth.** The console gives a per-class summary and one stack
trace; the XML gives every case, its duration and its failure. Two traps live in it — the
`time` attribute is written with the JVM's *default locale*, so on an Italian machine it
reads `0,123` and a strict parse reports every test as instantaneous; and a case that failed
and then passed on a rerun carries a `<flakyFailure>` and no `<failure>`, so counting the
suite's own attributes turns a green run red. Both are handled, and both have a test named
after the bug.

## Depends on

`bennu-facts` (the annotation-shaped Java scan), `bennu-complete` (byte offset → line),
`roxmltree`.
