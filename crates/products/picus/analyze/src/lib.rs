//! `picus-analyze` — the fourteen rules over a repository of SQL install
//! scripts, plus the declared suppressions that silence a finding without hiding
//! it.
//!
//! Pure, like the two crates it sits on: a `ParsedProject` and a `ProjectConfig`
//! in, a [`Report`] out. No filesystem, no clock, no database.
//!
//! ```ignore
//! use picus_analyze::prelude::*;
//! use picus_inventory::prelude::{Inventory, ParsedProject};
//!
//! let inventory = Inventory::build(&joined);
//! let report = analyze(&joined, &config, &inventory);
//!
//! report.findings;              // suppressed ones included, and marked
//! report.skipped;               // rules that could not run, and why
//! report.rejected_suppressions; // comments that silence nothing
//! ```
//!
//! Two things the rules are held to, and they are worth reading before adding a
//! fifteenth:
//!
//! * **`consequence` says what goes wrong in practice.** Never a restatement of
//!   the rule. A report whose messages are rule names is a report people learn to
//!   close.
//! * **A rule that cannot run says so.** It goes into `skipped` with a reason,
//!   because a rule that quietly passes for lack of input is indistinguishable
//!   from a rule that passed.
//!
//! [`Report`]: crate::report::Report

pub mod compare;
pub mod context;
pub mod finding;
pub mod prelude;
pub mod report;
pub mod rule;
pub mod rules;
pub mod suppress;

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
