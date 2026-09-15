//! `bennu-test` — running unit tests, as three pure problems.
//!
//! Pressing "run this test" looks like one action and is really three, each of which can be
//! got wrong independently:
//!
//! 1. **What is a test?** [`discover`] answers it from the source. Not from a naming
//!    convention — `*Test` is a Surefire default, not a definition, and a legacy tree is
//!    full of `FooTestCase`, `TestFoo` and helpers named `TestUtils` that are not tests at
//!    all. What makes a method a test is an annotation (`@Test` and its JUnit 5 relatives,
//!    or TestNG's), or, for JUnit 3, extending `TestCase` with a `testXxx()` method.
//! 2. **How do you ask Maven for exactly those?** [`selector`] builds the argument list.
//!    Surefire's `-Dtest` dialect has changed across versions and the modern spellings
//!    (fully-qualified names, `**` package globs) are silently ignored by the 2.x that a
//!    legacy project still pins — "silently" meaning it runs *everything* instead. So the
//!    selection is expressed the one way every version has understood: simple class names,
//!    comma-separated, with `Class#method` for a single case.
//! 3. **What happened?** [`surefire`] reads the XML reports back. This is the only source
//!    of per-case truth — the console prints a per-class summary and the failure text, but
//!    not which of the twelve cases in the class was the one that failed.
//!
//! Everything here is a function of its input. The process spawn, the event stream and the
//! report-directory watch live in `bennu-be`'s `tests` domain, which is where they can be
//! cancelled and where a partially-written file can simply be read again next tick.
//!
//! ## Two ecosystems, three problems each
//!
//! The same three questions have to be answered again for **Cargo**, and none of the answers
//! carries over — which is why they are separate files rather than branches inside these ones:
//!
//! | | Maven | Cargo |
//! |---|---|---|
//! | what is a test | [`discover`] (a class, via tree-sitter) | [`cargo_discover`] (a `fn`, via a scan) |
//! | how to ask for it | [`selector`] (`-Dtest=`) | [`cargo_selector`] (cargo flags **and** libtest filters) |
//! | what happened | [`surefire`] (XML on disk) | [`cargo_report`] (two output streams) |
//!
//! The shapes are genuinely different. Surefire's unit of report is a class and it writes a file;
//! libtest's is a case and it writes a line. Maven names a test one way; cargo splits the naming
//! between itself and the test binary. One thing IS shared, and deliberately: [`surefire::TestStatus`]
//! is the status vocabulary for both, so the panel has one set of icons and one meaning of red.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_test::prelude::...`.

pub mod cargo_discover;
pub mod cargo_report;
pub mod cargo_selector;
pub mod console;
pub mod discover;
pub mod prelude;
pub mod selector;
pub mod surefire;
