//! Canonical entry point for `bennu-java`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

// The type-inference entry points: the one-off caret query (parses + extracts) and the
// reuse-an-existing-tree variant for the hot reference-walk path.
pub use crate::infer::{
    enclosing_type_fqn, infer_expression_type, infer_expression_type_at,
    infer_expression_type_cached, infer_node_type_cached, infer_receiver_type,
    infer_receiver_type_at, infer_receiver_type_cached, InferCache, MethodResolution,
};
pub use crate::symbols::{extract_symbols, extract_symbols_from_root};

// The AST: the same parse read in Java's vocabulary, bodies included, typed where the resolver
// can say. Derived on demand and never stored — see `ast`'s module doc for why that is what makes
// it safe to have beside the declaration model rather than a second thing to keep in sync.
pub use crate::ast::{lower as lower_ast, AstNode};

// "Import class" detection: the simple type name under the caret that needs an import.
// The grammar itself, for callers that walk a parse rather than ask a question of it —
// the syntax-tree panel. One pin for the whole workspace (see `grammar.rs`).
pub use crate::grammar::{language as java_language, parse_java};
// Anonymous-class identity: the synthetic name an unnamed `new X() { … }` body is filed under,
// and the test that recognises one. Shared so the extractor and the caret query derive it the
// same way rather than each having its own idea.
pub use crate::symbols::{
    anonymous_supertype_name, anonymous_type_name, is_anonymous_body, parameter_name_node,
};
pub use crate::typename::{
    inherited_member_type, inherited_member_type_of, is_primitive, is_resolved_binary,
    java_lang_implicit, known_spelling, resolve_written_type, same_binary_type, NameScope,
    TypeName,
};

pub use crate::import_hint::simple_type_needing_import;

// Static-import targets — `import static …` parsed into (owner, member) for inference + undefined-var.
pub use crate::static_import::{static_import_targets, StaticImportTarget};

// The structural model produced by `extract_symbols`.
pub use crate::symbols::{
    collect_annotations, AnnString, Annotation, FieldDecl, FileSymbols, Import, MethodDecl,
    ParamDecl, Span, TypeDecl, TypeKind, ENUM_IMPLICIT_METHODS,
};

// The resolver seam the type-walk consumes + the member shapes it resolves against.
pub use crate::seam::{
    ClassFlags, ClassMembers, Member, MemberKind, TypeRef, TypeResolver, Visibility,
};

// New-file scaffolding: infer a Java package from a dir + render initial file content.
pub use crate::scaffold::{
    infer_package, java_template, package_dir, scaffold_new_file, source_root_of, NewFileKind,
    ScaffoldResult,
};

// Declaration-site name-span + binary-name CST scans (go-to-declaration / rename / inherited).
pub use crate::spans::{binary_of_type_at, enclosing_type_binary, find_type_name_span};
