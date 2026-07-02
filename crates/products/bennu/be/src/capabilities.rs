//! `capabilities` domain — `bennu_capabilities`.
//!
//! Re-run the Spike-D capability detection for a project without re-opening it (the
//! FE calls this to refresh the capability panel after, e.g., a pom edit). Thin
//! wrapper over `bennu-project`: parse the pom, detect, return the bitset.

use std::path::Path;

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::CapabilitySet;
use bennu_project::prelude::{detect_capabilities, parse_pom};
use serde::Deserialize;

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
    let root = Path::new(&args.root);
    let xml = std::fs::read_to_string(root.join("pom.xml")).unwrap_or_default();
    let pom = parse_pom(&xml);
    Ok(detect_capabilities(root, &pom))
}
