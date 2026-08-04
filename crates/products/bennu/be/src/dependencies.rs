//! `dependencies` domain — `bennu_dependencies` (the Dependencies tool window).
//!
//! Reads the project's poms and matches them against the dependency classpath the index service
//! **already** resolved. Deliberately never runs Maven itself: a tool window that shells out to a
//! build tool when you open it is a tool window nobody opens twice, and the resolve costs seconds.
//! When nothing has been resolved yet the report says so ([`Report::classpath_known`]), and the FE
//! distinguishes *unknown* from *missing* — showing every dependency of a project that builds fine
//! as "not resolved" would be worse than showing nothing.
//!
//! All the work is in the leaf `bennu-deps` crate; this module is the marshalling.

use bennu_core::prelude::BennuState;
use bennu_deps::prelude::{read as read_dependencies, Report};
use serde::Deserialize;

use crate::dep_classpath::cached_dep_jars;

/// Args for [`bennu_dependencies`].
#[derive(Deserialize)]
pub struct DependenciesArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
}

/// Every module's dependencies, with each version's origin and its resolved jar.
///
/// Never errors: a project with no `pom.xml`, an unreadable pom, an unresolved classpath — each is
/// a state the report describes rather than a failure the panel has to render as an error.
#[arbor_rpc::handler]
fn bennu_dependencies(_ctx: &BennuState, args: DependenciesArgs) -> Result<Report, String> {
    let root = std::path::PathBuf::from(&args.root);
    let jars = cached_dep_jars(&root);
    Ok(read_dependencies(&root, &jars))
}
