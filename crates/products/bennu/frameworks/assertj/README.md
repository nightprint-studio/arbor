# bennu-assertj

AssertJ, read for **the assertions that cannot fail** — and the Alt+Enter rewrites from JUnit's
`assertEquals` family to `assertThat`.

## The idea

A green test is the one nobody opens again, so a test that is green for the wrong reason is the most
expensive kind. AssertJ makes that easy to write:

```java
assertThat(order.total());                     // builds an assertion object, checks nothing
assertThat(order.total()).as("the total");      // still nothing

SoftAssertions softly = new SoftAssertions();
softly.assertThat(order.total()).isEqualTo(3);  // collected… and never reported
```

All three compile, run and pass whatever the total is. None of them looks wrong in review.

## Checks

| Code | Severity | What |
|---|---|---|
| `assertj.asserts-nothing` | warning | A statement whose AssertJ chain (`assertThat`, `assertThatObject`, `assertThatCode`, BDD `then…`, a soft-assertions object's `assertThat…`) has only configuring calls after it — `as`, `describedAs`, `extracting`, `filteredOn`, `usingComparator`, … Squiggles the statement. |
| `assertj.soft-assertions-never-asserted` | warning | A local `new SoftAssertions()` / `new BDDSoftAssertions()` the method only ever calls `assertThat…` / `then…` on — no `assertAll()`, and the variable goes nowhere else. Squiggles the name. |
| `assertj.use-dedicated-assertion` | weak | `assertThat(list.isEmpty()).isTrue()` and friends, which on failure can only say *expected true*. Squiggles the `assertThat` argument. |

## Rewrites (Alt+Enter)

| Id | Offered | Does |
|---|---|---|
| `assertj.add-assert-all` | on a `soft-assertions-never-asserted` finding | Appends `softly.assertAll();` as the method's last statement — only when no `return`, no lambda or anonymous class captures the variable, the variable is declared in the method's own block, and the last statement completes normally. |
| `assertj.use-dedicated-assertion` | on the finding, or anywhere in its statement | `a.equals(b)` → `isEqualTo(b)` / `isNotEqualTo(b)`; `x == null` → `isNull()` / `isNotNull()`; `x instanceof T` → `isInstanceOf(T.class)`; and, when the receiver's type is written in the file and is a JDK `String` / `CharSequence` / `StringBuilder` / collection / map / `Optional` / array: `isEmpty` / `isNotEmpty`, `hasSize(n)` from `size()` / `length()` / `.length`, `contains` / `doesNotContain`, `startsWith`, `endsWith`, `isPresent`. |
| `assertj.from-junit` | caret in a JUnit 4 (`org.junit.Assert`) or JUnit 5 (`org.junit.jupiter.api.Assertions`) assertion | `assertEquals(e, a)` → `assertThat(a).isEqualTo(e)`, and `assertNotEquals`, `assertSame`, `assertNotSame`, `assertArrayEquals` (→ `containsExactly`), `assertTrue`, `assertFalse`, `assertNull`, `assertNotNull`. A message becomes `.as(msg)` — JUnit 4's from the front, JUnit 5's from the back. Adds `import static org.assertj.core.api.Assertions.assertThat;` when needed. |
| `assertj.from-junit-file` | any caret, in a file with at least two convertible assertions | All of the above in one edit set, with the one import. |

Every call is **resolved through the file's imports** (`bennu_facts::static_call_resolves_to`), never
matched by name: Hamcrest's `assertThat(x, is(1))`, JUnit 4's, and a project's own helper are not
AssertJ's. A class implementing `WithAssertions` gets its bare `assertThat` recognised too.

## What it will not judge

- A soft-assertions object that goes **anywhere** — an argument, a return, an assignment — may be
  reported by whoever receives it. Auto-closing soft assertions, `assertSoftly(…)` and the JUnit 5
  extension report on their own and are never candidates.
- A type-dependent rewrite on a receiver whose type is not **written** where the name is declared
  (`var`, a lambda parameter, an inherited field, a method call) — or that is a project class named
  like a JDK one. `contains` on a collection needs the element to visibly have the element type.
- `assertThat(x instanceof T).isFalse()` — `isNotInstanceOf` fails on `null`, where the original holds.
- JUnit delta overloads, a `Supplier` message, a JUnit 4 two-argument `assertTrue(label, flag)` whose
  first argument is not a literal, a message literal containing `%`, `assertArrayEquals` over arrays
  whose written types are not visibly the same, and classes extending `TestCase` / `Assert`.
- A file where a bare `assertThat` already means something else: the import the rewrite needs would
  take the name away from it.
- A chain AssertJ can configure further than the list above knows (`usingRecursiveComparison()
  .ignoringFields(…)` alone) is read as a check — under-reported, never misreported.

## Public API

```rust
use bennu_assertj::prelude::*;
```

`AssertJExtension` (with `new()`), the codes `CODE_ASSERTS_NOTHING`, `CODE_SOFT_NEVER_ASSERTED`,
`CODE_DEDICATED`, and the intention ids `INTENTION_ADD_ASSERT_ALL`, `INTENTION_DEDICATED`,
`INTENTION_FROM_JUNIT`, `INTENTION_FROM_JUNIT_FILE`.
