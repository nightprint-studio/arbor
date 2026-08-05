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
