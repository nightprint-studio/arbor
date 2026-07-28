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
}
