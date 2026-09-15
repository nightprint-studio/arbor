# bennu-mockito

Mockito used wrongly, caught in the test that is wrong — as a **framework extension** on the
[`bennu-ext`](../ext) seam.

```rust
use bennu_mockito::prelude::*;
use bennu_ext::prelude::*;

let ext = MockitoExtension::new();          // applies only where Mockito is on the classpath
ext.diagnostics(&ctx);                      // unfinished stubbings, mixed matchers, uninitialised @Mock
ext.intentions(&ctx, offset, &problems);    // wrap in eq(…), add the extension
```

## The idea

Mockito keeps its state in a thread-local and notices most misuse only when that state is next
looked at — by the **next** Mockito call. So the exception names a line that is fine, often in a
different test method. The broken line compiles and reads naturally. The defect is local; the report
of it is not. That is the whole case for reading it statically.

## What is checked

| Code | Severity | Example | Mockito throws |
|---|---|---|---|
| `mockito.unfinished-stubbing` | error | `when(repo.find(1));` · `doReturn(x).when(repo);` | `UnfinishedStubbingException`, at the next Mockito call |
| `mockito.unfinished-verification` | error | `verify(repo);` · `then(repo).should();` | `UnfinishedVerificationException`, at the next Mockito call |
| `mockito.mixed-matchers` | error | `verify(repo).save(any(), 5)` | `InvalidUseOfMatchersException` |
| `mockito.mocks-not-initialised` | warning | `@Mock Repo repo;` in a test with no extension, runner or `openMocks` | nothing — the field is null, and the first stubbing is a `NullPointerException` |

Two fixes, both recomputed from the buffer (a squiggle the analysis no longer agrees with gets none):

- **`mockito.wrap-in-eq`** — every plain argument of the call wrapped in `eq(…)`. Spelled bare when
  `eq` is reachable, qualified like the call's own matchers when they are written qualified
  (`ArgumentMatchers.eq`), otherwise bare with `import static org.mockito.ArgumentMatchers.eq;`.
- **`mockito.add-extension`** — `@ExtendWith(MockitoExtension.class)` on the class (JUnit 5) or
  `@RunWith(MockitoJUnitRunner.class)` (JUnit 4), at the class's own indentation, with the imports.

## How it stays right

**Every name is resolved, never matched.** `when`, `verify`, `any`, `not` are ordinary identifiers;
a test helper, AssertJ and Hamcrest all declare some of them. Each call is resolved through the
file's imports with `bennu-facts`' `static_call_resolves_to`, and a bare name is trusted only when no
other static on-demand import could declare it too — except the usual neighbours (JUnit's
`Assert`/`Assertions`, AssertJ's `Assertions`), whose overlapping names are listed.

**A plain value is a certainty, not a look.** Mockito counts matchers *registered while the arguments
were evaluated*, so anything that might register one makes the count unknowable and the call silent:
any method call inside an argument (`orderWith(id)`, `captor.capture()`, `order.getId()`), a parameter
of a helper method (the caller may have passed `any()`), a lambda parameter, a local assigned from a
call, a matcher that is not certainly Mockito's.

**Uninitialised mocks are reported only where the class says plainly that nothing initialises them:**
a JUnit 4 or JUnit 5 file (not TestNG, not both), a concrete class with tests of its own, no
`extends`, no `implements`, no class-level annotation outside a short inert list (`@DisplayName`,
`@Tag`, `@Nested`, `@TestInstance`, …) — on the class and on every class around it — and nothing in
the file that looks like initialisation: `openMocks`, `MockitoAnnotations`, `MockitoJUnit`, a
Mockito session, a `@Rule` or `@RegisterExtension`, a call passed `this`, or the field assigned by
hand.

## What it will not judge

- A result that is assigned or returned — `var s = when(x.m());` may be finished elsewhere.
- Deep stubs (`when(repo.child().find(any(), 1))`), `lenient().when(…)`, BDD's `willReturn(…).given(…)`.
- Matchers nested inside a plain value, or any argument it cannot prove plain (see above).
- Mockito's other misuse: stubbing a `void` or `final` method, a matcher outside a stubbing.
- Initialisation it cannot see: a superclass, an interface carrying `@ExtendWith`, a composed or
  Spring annotation, an extension registered globally through the JUnit service loader.
- A `when` inherited from a test base class — invisible to import resolution, and the one way a
  helper could be mistaken for Mockito's.

## Public API

Through the [`prelude`](src/prelude.rs): `MockitoExtension`, the four `CODE_*` diagnostic codes and
the two `INTENTION_*` ids. Everything else is internal.
