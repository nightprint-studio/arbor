//! `bennu-assertj` — the assertions that cannot fail.
//!
//! ## A passing test is not evidence
//!
//! AssertJ's fluency has a price nobody sees in review: `assertThat(total)` is an *assertion object*,
//! not an assertion. Nothing is checked until a method like `isEqualTo` is called on it, so
//!
//! ```java
//! assertThat(order.total());
//! assertThat(order.total()).as("the total");
//! ```
//!
//! compile, run, and pass whatever the total is. The same goes for `SoftAssertions`: the failures are
//! collected, and if nobody calls `assertAll()` they are collected into nothing. Both are tests that
//! are green for the wrong reason, and a green test is the one nobody opens again.
//!
//! ## What this crate says
//!
//! 1. [`CODE_ASSERTS_NOTHING`] — a statement whose AssertJ chain ends before any check.
//! 2. [`CODE_SOFT_NEVER_ASSERTED`] — a local soft-assertions object nothing ever reports, with the
//!    fix that adds the `assertAll()`.
//! 3. [`CODE_DEDICATED`] — `assertThat(list.isEmpty()).isTrue()`, which on failure can only say
//!    *expected true*; `assertThat(list).isEmpty()` shows the list.
//! 4. The rewrites from JUnit's `assertEquals` family to `assertThat`, one at a time or for the whole
//!    file.
//!
//! Every name is **resolved** rather than matched: `assertThat` is also Hamcrest's, JUnit 4's, and
//! whatever a project's test helper is called, and a check that mistook one of those for AssertJ's
//! would be wrong about exactly the code it exists to protect.
//!
//! [`CODE_ASSERTS_NOTHING`]: ext::CODE_ASSERTS_NOTHING
//! [`CODE_SOFT_NEVER_ASSERTED`]: ext::CODE_SOFT_NEVER_ASSERTED
//! [`CODE_DEDICATED`]: ext::CODE_DEDICATED

// The chain of calls a statement is made of: where it starts, what follows.
mod chain;
// `assertThat(list.isEmpty()).isTrue()` and the assertion that says what it means.
mod dedicated;
// The extension itself — what a host registers.
pub mod ext;
// JUnit's assertions, rewritten as AssertJ's.
mod junit;
// The chain with no check at the end.
mod nothing;
pub mod prelude;
// Whether a call is AssertJ's (or JUnit's), through the file's imports.
mod resolve;
// Which declaration a name refers to at a point in the file.
mod scope;
// What a declared type is, as far as a rewrite needs to know.
mod shape;
// Soft assertions nobody reports.
mod soft;
// Small tree-sitter helpers every module needs.
mod syntax;
// One buffer, parsed once.
mod unit;

#[cfg(test)]
mod testing;
