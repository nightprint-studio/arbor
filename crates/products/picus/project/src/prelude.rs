//! Canonical entry point for `picus-project`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_project::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the diff always goes through here.

pub use crate::config::{
    BranchConfig, EncodingSettings, FolderConfig, GenerationSettings, ProjectConfig,
    VersionTableSettings, CURRENT_VERSION, DEFAULT_ENCODING, PROJECT_CONFIG_RELATIVE_PATH,
};
pub use crate::discover::{
    discover, label_to_encoding, plan, scan, Proposal, ProposalNote, SourceFile, SAMPLE_BYTES,
    SCRIPT_EXTENSIONS,
};
pub use crate::error::ProjectError;
pub use crate::infer::{infer_dialect, infer_role, Guess};
pub use crate::insertion::InsertionRule;
pub use crate::marker::{MarkerFields, MarkerTemplate, DEFAULT_MARKER, KNOWN_PLACEHOLDERS};
pub use crate::naming::{CompiledNaming, NamingScheme, VersionRange};
pub use crate::tree::{Branch, LineEnding, Project, ScriptFile, ScriptFolder};
pub use crate::version::Version;

// Re-exported so a consumer working in project terms does not have to name the
// leaf crate for the two types that are unavoidably part of this vocabulary.
pub use picus_types::prelude::{EngineKind, FolderRole};
