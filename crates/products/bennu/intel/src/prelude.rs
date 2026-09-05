//! Canonical entry point for `bennu-intel`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_intel::prelude::...`. The submodule stays `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::provider::{
    declarable_type_at, declarable_type_detail, render_type_for_source, Declarable, DocumentSymbol, IntelError, IntelProvider, LibraryMember, LibraryTarget,
    Location, LspClientProvider, NativeJavaProvider, Position, ProjectMember, TextEdit,
};

// The class-name index behind the "Import class" intention (simple name → importable FQNs).
pub use crate::class_names::{ClassNameIndex, Segment};

// The wire types the provider produces, re-exported so a consumer (bennu-be) reaches
// them through the intel prelude it already imports.
pub use bennu_proto::prelude::{CompletionItem, Diagnostic};

// The config-graph integration: ingest the Struts/Spring/Tiles graph into the index +
// resolve the C1 chain, the view chain, and the conservative action diagnostic.
pub use crate::config::{
    action_qname, ingest_config_graph, ActionTarget, ActionVerdict, ConfigResolver,
};

// Spring stereotype-bean policy: the annotation-declared beans (`@Service`/`@Component`/…) the
// config resolver consults as a C1 fallback, plus the project-scan collector the be build calls.
pub use crate::spring_beans::{collect_annotation_beans, stereotype_bean, AnnotationBean};
// MyBatis go-to / outline wire types, re-exported so the be-layer maps them without
// depending on `bennu_web` directly.
pub use bennu_web::prelude::{StatementKind, StatementRecord, StatementTarget};

// Cross-file references / find-usages + the caret classifier (docs §5 #7).
pub use crate::refs::{
    build_reference_index, build_reference_index_incremental, classify_caret, classify_target,
    references, AliasUsages, DeclKey, IncrementalBuild, LangLevel, ReferenceIndex, ReferencesResult,
    RenameTarget, SourceFile, UsageLocation,
};

// Persisted, incremental reference-index cache: the be layer clears it on a manual "Rebuild
// index" (a clean full walk); the engine's `for_project` loads / saves it internally.
pub use crate::refcache::{cache_path as ref_cache_path, clear as clear_ref_cache};

// Persisted, dependency-aware DIAGNOSTIC cache: makes re-validating an unchanged project (or the
// unchanged part of an edited one) instant. The be layer's whole-project validation loads it,
// serves fresh entries, stores fresh ones, and persists it; it's cleared on a manual "Rebuild
// index" like the reference cache.
pub use crate::diag_cache::{
    cache_path as diag_cache_path, clear as clear_diag_cache, load as load_diag_cache,
    save as save_diag_cache, source_hash, CacheEntry, DiagCache, FileDeps,
};
// Re-surfaced from bennu-query so the be layer reaches recording through the intel prelude it
// already imports (the provider's `validate_recording` returns these).
pub use bennu_query::prelude::{ProjectView, RecordedDeps};

// RENAME planning + apply (docs §5 #10-12): best-effort, preview-first, config-aware.
// Plus go-to-declaration (`resolve_declaration` + `DeclarationLocation`), which reuses the
// same caret classifier + decl-site name-span finders.
pub use crate::rename::{
    file_rename_for, find_member_name_span, find_member_name_spans, plan_types, rename_apply,
    rename_plan, resolve_declaration, DeclarationLocation, Edit, EditReason, FileEdits, FileRename,
    HoverInfo, RenamePlan, SubtypeMap, TypeRename,
};
// The project's semantic model — what every whole-project question is answered from. Consumers hold
// one of these per open project; the free functions above are how it answers.
pub use crate::engine::SemanticEngine;
// The rename planner's project-source input unit + the type-declaration name-span finder now live
// in the base crates (`bennu-query` / `bennu-java`); re-surfaced here as part of the rename API.
pub use bennu_java::prelude::find_type_name_span;
pub use bennu_query::prelude::PlanFile;
// The per-file encoding map the index readers decode through — re-surfaced so a caller that reads
// sources needs one import, not two.
pub use bennu_project::prelude::EncodingPlan;

// Spell-check engine (declaration names + comments): the pure tokenizer / allow-list +
// the process-wide dictionary cache + the Java-source walk.
pub use crate::spell::{
    global_custom_dict_path, installed_languages, is_tech_allowed, is_trivially_skippable,
    project_custom_dict_path, tokenize_identifier, SpellEngine, SpellHit, SubWord, TECH_ALLOWLIST,
};

// Inherited ("super") members + the resolver + member-access completion live in `bennu-query`;
// consumers (provider / be / tests) import them from `bennu_query::prelude` directly (clean cut,
// no facade re-export).
pub use crate::java_index::{
    background_workers, build_project_index, build_project_index_from_sources, collect_java,
    file_records_from_source, parallel_map, parallel_map_capped, project_type_map,
    read_java_sources, read_source_for_index, set_background_workers, set_excluded_dirs, ClassDecl,
    NonCompliantSource, ProjectBuild, ProjectSources,
};

// A field is also known by the accessors nobody wrote — what a rename must carry, and what
// find-usages must find. One answer for both.
pub use crate::rename::{generated_aliases, FieldAlias};

// Call / type hierarchy over the Java engine — the same reference index find-usages reads, walked
// as a tree. Entered through `SemanticEngine::prepare_hierarchy` / `hierarchy_step`; the types are
// here because the be layer maps them onto the wire.
pub use crate::hierarchy::{
    HierarchyCallSite, HierarchyDirection, HierarchyHandle, HierarchyItem,
};

pub use crate::safe_delete::{safe_delete_plan, SafeDelete};
