//! `dependencies` domain — `bennu_dependencies` (the Dependencies tool window).
//!
//! One handler, two ecosystems, one report shape:
//!
//! - a **Maven** root: the reactor's poms, matched against the dependency classpath the index service
//!   **already** resolved;
//! - a **Cargo** root: the workspace's manifests, with the versions `Cargo.lock` actually chose and
//!   the unpacked sources in the local registry.
//!
//! Deliberately never runs the build tool: a tool window that shells out when you open it is a tool
//! window nobody opens twice, and both resolves cost seconds. When nothing has been resolved yet the
//! report says so ([`Report::resolved_known`]), and the FE distinguishes *unknown* from *missing* —
//! showing every dependency of a project that builds fine as "not resolved" would be worse than
//! showing nothing.
//!
//! All the work is in the leaf `bennu-deps` crate; this module is the marshalling and the choice of
//! which producer answers.

use bennu_core::prelude::BennuState;
use bennu_deps::prelude::{read as read_dependencies, read_cargo, Report};
use serde::Deserialize;

use crate::dep_classpath::cached_dep_jars;

/// Args for [`bennu_dependencies`].
#[derive(Deserialize)]
pub struct DependenciesArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml` / `Cargo.toml`).
    pub root: String,
}

/// Every module's dependencies, with each version's origin and where the artifact actually is.
///
/// Never errors: a project with no manifest, an unreadable one, an unresolved classpath, a missing
/// `Cargo.lock` — each is a state the report describes rather than a failure the panel has to render
/// as an error.
#[arbor_rpc::handler]
fn bennu_dependencies(_ctx: &BennuState, args: DependenciesArgs) -> Result<Report, String> {
    let root = std::path::PathBuf::from(&args.root);
    // Maven first when both are there, which is the precedence `bennu-project`'s `open_project`
    // applies: a polyglot root is the Java project, and the panel has to agree with the rest of the
    // window about what it is looking at.
    if root.join("pom.xml").is_file() || !root.join("Cargo.toml").is_file() {
        let jars = cached_dep_jars(&root);
        return Ok(read_dependencies(&root, &jars));
    }
    Ok(read_cargo(&root))
}
