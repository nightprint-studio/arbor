//! The fourteen rules, one module per family.
//!
//! Each `run` takes the resolved [`Context`] and appends to the shared
//! [`Output`]. No rule reads another's findings — a rule that depended on
//! whether a neighbour fired would be a rule whose behaviour changed when the
//! neighbour's threshold moved.
//!
//! The two consistency modules are the two axes a repository can drift along,
//! and keeping them apart is the point: [`consistency`] compares one dialect
//! against the other, [`propagation`] compares one dialect's initialisation
//! against its own updates.

use crate::context::Context;
use crate::report::Output;
use crate::rule::RuleId;

pub mod consistency;
pub mod dialect;
pub mod dml;
pub mod duplicate;
pub mod encoding;
pub mod propagation;
pub mod version;

pub(crate) fn run_all(context: &Context<'_>, output: &mut Output) {
    consistency::run(context, output);
    propagation::run(context, output);
    dialect::run(context, output);
    version::run(context, output);
    duplicate::run(context, output);
    encoding::run(context, output);
    dml::run(context, output);
    withhold_disabled(context, output);
}

/// Drop what the project has said it does not want to see, and say so.
///
/// Filtered here, once, rather than checked inside each rule. A rule that had to
/// remember to ask would eventually be a rule that forgot, and forgetting shows up
/// as a report full of exactly the findings somebody already switched off — which
/// is how a person learns to stop reading the report.
///
/// Every disabled rule leaves a line in `skipped`, and that is the load-bearing
/// half. A rule that produced nothing because it was told not to run must not be
/// indistinguishable from one that ran and found nothing; the whole value of this
/// product is that a clean report means something.
///
/// The skip written here **replaces** any the rule wrote for itself: a rule can
/// have several reasons not to run at once, and "you turned it off" is the only
/// one the reader can act on.
fn withhold_disabled(context: &Context<'_>, output: &mut Output) {
    let disabled: Vec<RuleId> = RuleId::ALL.into_iter().filter(|r| context.is_disabled(*r)).collect();
    if disabled.is_empty() {
        return;
    }
    output.findings.retain(|f| !disabled.contains(&f.rule));
    output.skipped.retain(|s| !disabled.contains(&s.rule));
    for rule in disabled {
        output.skip(
            rule,
            "",
            "this project switches the rule off — see `[analysis] disabled_rules` in \
             `.arbor/picus/project.toml`, or the Rules section of the project settings",
        );
    }
}
