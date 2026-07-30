//! `picus-diff` — the comparison engine.
//!
//! Two schemas, two sets of counts or two sets of rows in; one [`DiffReport`]
//! out. It opens nothing, reads nothing and waits for nothing.
//!
//! ```ignore
//! use picus_diff::prelude::*;
//!
//! let config = DiffTemplates::builtin().config_for("structure");
//! let mut report = DiffReport::new("production", "staging");
//!
//! report.schema      = Some(compare_schema(&snapshot_a, &snapshot_b, &config));
//! report.indexes     = Some(compare_indexes(&snapshot_a, &snapshot_b, &config));
//! report.constraints = Some(compare_constraints(&snapshot_a, &snapshot_b, &config));
//! if !config.contents.enabled {
//!     report.skip(CheckKind::Contents, SkipReason::Disabled, "contents are off in this template");
//! }
//!
//! let report = report.finish();   // ← computes the verdict
//! ```
//!
//! ## Why it is pure
//!
//! Not for tidiness. The same engine has to answer three questions:
//!
//! 1. **database against database** — two snapshots read over two connections;
//! 2. **database against the scripts that install it** — one snapshot read from a
//!    server, the other *derived from a repository of SQL files* that no
//!    connection was ever opened to;
//! 3. **one query's results against another's** — two [`RowSet`]s that never had
//!    a schema.
//!
//! An engine that knew how to connect could only do the first. Taking structures
//! that are already read is what makes the second possible at all, and it is also
//! what lets every rule in here be tested against a fixture instead of a
//! database — which is why the test module can assert on composite keys, on
//! `1` versus `1.0`, and on a threshold, without anything running.
//!
//! ## The two things it refuses to do
//!
//! **It does not normalise.** A type is compared as each server spelled it, a
//! value as the type it came back as. Both are places where "being helpful" turns
//! into telling somebody their correct script is broken, or hiding that their
//! broken one is.
//!
//! **It does not round up to "identical".** A check that did not run is recorded
//! and the verdict says so — see [`report`].
//!
//! ## Public API: use the [`prelude`]
//!
//! [`DiffReport`]: crate::report::DiffReport
//! [`RowSet`]: crate::rows::RowSet

pub mod change;
pub mod config;
pub mod counts;
pub mod error;
pub mod names;
pub mod prelude;
pub mod report;
pub mod rows;
pub mod schema;
pub mod template;
pub mod value;

#[cfg(test)]
mod tests;
