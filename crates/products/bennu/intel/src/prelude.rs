//! Canonical entry point for `bennu-intel`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intel::prelude::...`. The submodule stays `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::provider::{
    DocumentSymbol, IntelError, IntelProvider, Location, LspClientProvider, NativeJavaProvider,
    Position, ProjectMember, TextEdit,
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
    build_reference_index, build_reference_index_incremental, classify_caret, classify_target,
    references, DeclKey, IncrementalBuild, ReferenceIndex, ReferencesResult, RenameTarget,
    SourceFile, UsageLocation,
};

// Persisted, incremental reference-index cache: the be layer clears it on a manual "Rebuild
// index" (a clean full walk); the engine's `for_project` loads / saves it internally.
pub use crate::refcache::{cache_path as ref_cache_path, clear as clear_ref_cache};

// RENAME planning + apply (docs §5 #10-12): best-effort, preview-first, config-aware.
// Plus go-to-declaration (`resolve_declaration` + `DeclarationLocation`), which reuses the
// same caret classifier + decl-site name-span finders.
pub use crate::rename::{
    find_member_name_span, find_type_name_span, rename_apply, rename_plan, resolve_declaration,
    DeclarationLocation, Edit, EditReason, FileEdits, HoverInfo, PlanFile, RenameEngine, RenamePlan,
};

// Spell-check engine (declaration names + comments): the pure tokenizer / allow-list +
// the process-wide dictionary cache + the Java-source walk.
pub use crate::spell::{
    global_custom_dict_path, installed_languages, is_tech_allowed, is_trivially_skippable,
    project_custom_dict_path, tokenize_identifier, SpellEngine, SpellHit, SubWord, TECH_ALLOWLIST,
};

// Inherited ("super") members of a type — the Structure panel's lazy "Inherited" bucket.
// Reuses the resolver's supertype walk (superclass + interfaces), one level up from the
// type's own members.
pub use crate::inherited::{inherited_members, InheritedMember, InheritedSource};

// The Phase-1 completion machinery, for the be layer to build a project's provider.
pub use crate::completion::completion;
pub use crate::java_index::{
    build_project_index, build_project_index_from_sources, collect_java, file_records_from_source,
    project_type_map, read_java_sources, read_source_for_index, ClassDecl, NonCompliantSource,
    ProjectBuild, ProjectSources,
};
pub use crate::jdk::JdkMemberIndex;
pub use crate::resolver::{convert_members, IndexResolver};
