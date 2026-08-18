//! Canonical entry point for `bennu-test`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_test::prelude::...`. The submodules stay `pub` for rustdoc navigation, but the
//! prelude is the canonical call-site path.

// Discovery: what in a file is a test, and where.
pub use crate::discover::{
    discover_in_source, discover_tests, TestClass, TestFramework, TestMethod, TEST_MARKERS,
};

// Selection: the Maven command line for a chosen scope.
pub use crate::selector::{plan, MavenPlan, TestCaseRef, TestScope};

// Results: the Surefire report read back.
pub use crate::surefire::{parse_report, TestCaseResult, TestClassResult, TestStatus};

// The two console lines worth reading while the run streams.
pub use crate::console::{run_totals, running_class, RunTotals};

// ── Cargo ─────────────────────────────────────────────────────────────────────

// Discovery: which `fn` is a test, and where it sits in the build.
pub use crate::cargo_discover::{
    discover_rust_in_source, place_of, FilePlace, RustTest, RustTestKind, TestTarget,
};

// Selection: the `cargo test` command line for a chosen scope.
pub use crate::cargo_selector::{cargo_plan, CargoPlan, CargoTestScope, RustCaseRef};

// Results: the run's two output streams, read line by line.
pub use crate::cargo_report::{
    compiling_crate, running_target, LibtestEvent, LibtestParser, LibtestResult, RunningTarget,
};
