//! Canonical entry point for `bennu-intel`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intel::prelude::...`. The submodule stays `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::provider::{
    DocumentSymbol, IntelError, IntelProvider, Location, LspClientProvider, NativeJavaProvider,
    Position, TextEdit,
};

// The wire types the provider produces, re-exported so a consumer (bennu-be) reaches
// them through the intel prelude it already imports.
pub use bennu_proto::prelude::{CompletionItem, Diagnostic};

// The config-graph integration: ingest the Struts/Spring/Tiles graph into the index +
// resolve the C1 chain, the view chain, and the conservative action diagnostic.
pub use crate::config::{
    action_qname, ingest_config_graph, ActionTarget, ActionVerdict, ConfigResolver,
};

// Cross-file references / find-usages + the caret classifier (docs §5 #7).
pub use crate::refs::{
    build_reference_index, classify_caret, classify_target, references, DeclKey, ReferenceIndex,
    ReferencesResult, RenameTarget, SourceFile, UsageLocation,
};

// RENAME planning + apply (docs §5 #10-12): best-effort, preview-first, config-aware.
pub use crate::rename::{
    rename_apply, rename_plan, Edit, EditReason, FileEdits, PlanFile, RenameEngine, RenamePlan,
};

// The Phase-1 completion machinery, for the be layer to build a project's provider.
pub use crate::completion::completion;
pub use crate::java_index::{
    build_project_index, collect_java, file_records_from_source, project_type_map,
};
pub use crate::jdk::JdkMemberIndex;
pub use crate::resolver::{convert_members, IndexResolver};
