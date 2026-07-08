//! Canonical entry point for `bennu-java`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

// The type-inference entry points: the one-off caret query (parses + extracts) and the
// reuse-an-existing-tree variant for the hot reference-walk path.
pub use crate::infer::{
    infer_expression_type, infer_expression_type_at, infer_expression_type_cached,
    infer_node_type_cached, infer_receiver_type, infer_receiver_type_at, infer_receiver_type_cached,
    InferCache, MethodResolution,
};
pub use crate::symbols::{extract_symbols, extract_symbols_from_root};

// "Import class" detection: the simple type name under the caret that needs an import.
pub use crate::import_hint::simple_type_needing_import;

// Static-import targets — `import static …` parsed into (owner, member) for inference + undefined-var.
pub use crate::static_import::{static_import_targets, StaticImportTarget};

// The structural model produced by `extract_symbols`.
pub use crate::symbols::{
    Annotation, FieldDecl, FileSymbols, Import, MethodDecl, ParamDecl, TypeDecl, TypeKind,
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
pub use crate::spans::{binary_of_type_at, find_type_name_span};
