//! `capabilities` domain — `bennu_capabilities`, and the one place the backend gathers what
//! capability detection reads.
//!
//! Re-run the Spike-D capability detection for a project without re-opening it (the FE calls this
//! to refresh the capability panel after, e.g., a pom edit). The framework registry asks
//! [`project_capabilities`] too, so the panel and the extensions that actually run cannot disagree
//! about what a project has.

use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use bennu_maven::prelude::LocalRepo;
use bennu_proto::prelude::CapabilitySet;
use bennu_project::prelude::{detect_capabilities_with, parse_pom, BuildEvidence};
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_capabilities`].
#[derive(Deserialize)]
pub struct CapabilitiesArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
}

/// Detect the domain capabilities (Spike-D ruleset) for the project at `root`. An
/// absent / unreadable pom yields an empty bitset (no hard-fail — detection is
/// evidence-based).
#[arbor_rpc::handler]
fn bennu_capabilities(_ctx: &BennuState, args: CapabilitiesArgs) -> Result<CapabilitySet, String> {
    Ok(project_capabilities(Path::new(&args.root)))
}

/// The capabilities of the project at `root`: every pom of its reactor, every module's config files
/// and sources, and the dependency tree Maven resolved.
///
/// The tree is not a refinement, it is where most of the evidence is. A capability is recognised by
/// an API artifact, and an API is almost never what a pom declares: `jakarta.validation-api` comes
/// in through `spring-boot-starter-validation`, a company parent, or a library that validates its
/// own beans. Before the tree was read, Bean Validation was off on projects full of `@NotNull`.
pub(crate) fn project_capabilities(root: &Path) -> CapabilitySet {
    let xml = std::fs::read_to_string(root.join("pom.xml")).unwrap_or_default();
    let coordinates = resolved_dependencies(root).into_iter().map(|(group, artifact, _)| format!("{group}:{artifact}"));
    let build = BuildEvidence::read(root, &parse_pom(&xml)).with_resolved(coordinates);
    detect_capabilities_with(&build)
}

/// `groupId:artifactId` of every jar the build resolved to.
///
/// The open project's list first: it is the classpath as it is, including a partial resolve that
/// the on-disk cache declines to keep. The cache only for a root no project slot holds. Empty until
/// Maven has resolved once — detection then answers from the poms, and is asked again when the tree
/// lands (`FrameworkService::reevaluate_capabilities`).
/// Every dependency on the resolved classpath as `(group, artifact, version)` — what the capabilities are
/// recognised by, and what a code template reads as `project.dependencies`.
pub(crate) fn resolved_dependencies(root: &Path) -> Vec<(String, String, String)> {
    let live = IndexService::global().dep_jars_of(&root.display().to_string());
    let jars: Vec<PathBuf> = match live.is_empty() {
        true => crate::dep_classpath::cached_dep_jars(root),
        false => live.into_iter().map(PathBuf::from).collect(),
    };
    if jars.is_empty() {
        return Vec::new();
    }
    let repo = LocalRepo::discover();
    jars.iter()
        .filter_map(|jar| repo.coord_at(jar))
        .map(|coord| (coord.group_id, coord.artifact_id, coord.version))
        .collect()
}
