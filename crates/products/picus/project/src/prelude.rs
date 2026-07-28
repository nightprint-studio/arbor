//! Canonical entry point for `picus-project`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_project::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the diff always goes through here.

pub use crate::alias::{alias_key, name_matches, AliasVocabulary, InferenceAlias};
pub use crate::config::{
    EncodingSettings, FolderDeclaration, GenerationSettings, ProjectConfig, VersionTableSettings,
    CURRENT_VERSION, DEFAULT_ENCODING, PROJECT_CONFIG_RELATIVE_PATH,
};
pub use crate::discover::{
    discover, label_to_encoding, plan, scan, Proposal, ProposalNote, SourceFile, SAMPLE_BYTES,
    SCRIPT_EXTENSIONS,
};
pub use crate::error::ProjectError;
pub use crate::infer::{infer_engine, infer_engine_in, infer_role, infer_role_in, Guess};
pub use crate::insertion::InsertionRule;
pub use crate::marker::{MarkerFields, MarkerTemplate, DEFAULT_MARKER, KNOWN_PLACEHOLDERS};
pub use crate::naming::{CompiledNaming, NamingScheme, VersionRange};
pub use crate::path::{contains, last_segment, parent_of, self_and_ancestors};
pub use crate::resolve::resolve;
pub use crate::tree::{FolderNode, LineEnding, Project, ScriptFile, Walk};
pub use crate::version::Version;

// Re-exported so a consumer working in project terms does not have to name the
// leaf crate for the two types that are unavoidably part of this vocabulary.
pub use picus_types::prelude::{EngineKind, FolderEngine, FolderRole, ForeignEngine};
