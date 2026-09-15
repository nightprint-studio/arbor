//! Canonical entry point for `bennu-java`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_java::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

// The type-inference entry points: the one-off caret query (parses + extracts) and the
// reuse-an-existing-tree variant for the hot reference-walk path.
pub use crate::infer::is_inferred_type;
// Overload applicability (JLS §15.12.2 in miniature, one copy): the overload a call binds to — at a
// caret, or for a call node and candidate set in hand — and the full verdict for a check that must
// tell "ambiguous" from "nothing applies". `arity_admits` is the count rule alone.
pub use crate::infer::{arity_admits, bound_overload, overload_fit, subtype_verdict, OverloadFit};
pub use crate::infer::{
    call_overload_at, enclosing_type_fqn, functional_descriptor, infer_expression_type, infer_expression_type_at,
    infer_expression_type_cached, infer_node_type_cached, infer_receiver_type, FunctionalDescriptor,
    infer_receiver_type_at, infer_receiver_type_cached, method_admits_argc, type_decl_at,
    InferCache,
    MethodResolution,
};
pub use crate::annotation_site::{annotation_site, target_admits, AnnotationSite, ElementTarget};
pub use crate::symbols::{extract_symbols, extract_symbols_from_root};

// The AST: the same parse read in Java's vocabulary, bodies included, typed where the resolver
// can say. Derived on demand and never stored — see `ast`'s module doc for why that is what makes
// it safe to have beside the declaration model rather than a second thing to keep in sync.
pub use crate::ast::{lower as lower_ast, AstNode};

// "Import class" detection: the simple type name under the caret that needs an import.
// The grammar itself, for callers that walk a parse rather than ask a question of it —
// the syntax-tree panel. One pin for the whole workspace (see `grammar.rs`).
pub use crate::grammar::{language as java_language, parse_java};
// The `/** … */` above a declaration — for one offset, or for every declaration in a file (which is
// how a library's documentation is read out of its `-sources.jar`).
pub use crate::javadoc::{
    declarations as javadoc_declarations, leading as leading_javadoc, FileDocs,
};
// Anonymous-class identity: the synthetic name an unnamed `new X() { … }` body is filed under,
// and the test that recognises one. Shared so the extractor and the caret query derive it the
// same way rather than each having its own idea.
pub use crate::symbols::{
    anonymous_supertype_name, anonymous_type_name, is_anonymous_body, parameter_name_node, parameter_type_node,
};
pub use crate::typename::{
    declared_type_in_scope, erase_type_arguments, inherited_member_type, inherited_member_type_of, is_primitive,
    bind_simple_name, is_resolved_binary, java_lang_implicit, scope_candidates, simple_name_reaches,
    split_array_dims, known_spelling, ScopeCandidate, ScopeKind,
    resolve_written_type, same_binary_type, NameScope, TypeName,
};

pub use crate::import_hint::simple_type_needing_import;

// What the lexical scope at a caret binds — locals, parameters, pattern variables. The other
// direction of the inference walk: what names are there, rather than what type does this one have.
pub use crate::scope::{caret_is_static, visible_bindings, Binding};

// The type a position wants — the strongest completion signal there is, and the one that is
// about the hole rather than about the candidate.
pub use crate::expected::{expected_type, expected_type_at, functional_descriptor_at};

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
pub use crate::spans::{binary_of_type_at, call_arity_at, enclosing_type_binary, find_type_name_span};
// A type declaration located by BINARY name, nesting included (`Outer$Inner` is `Inner` inside
// `Outer`, never a same-named type elsewhere in the file).
pub use crate::type_decl::{
    binary_simple_name, declared_package, declared_type_binary, find_binary_type_name_span, find_type_declaration,
    is_type_declaration, type_nesting, TYPE_DECLARATION_KINDS,
};
// Which declaration of an overloaded name takes a member's parameters — shared by go-to, rename and
// library Javadoc so they cannot disagree about which overload is which.
pub use crate::param_shape::{
    choose_overload, declared_parameter_shapes, parameter_shapes_match, ParamShape,
};

// The shared supertype walk — see `crate::hierarchy` for why there is only one.
pub use crate::hierarchy::{
    declaring, declaring_field, declaring_method, seen_as, substitute, supertype_names, walk,
    walk_up, Ancestor, Walk, MAX_HIER_NODES,
};

// Reading a declaration the way a framework rule needs to: its annotations, its modifiers as
// tokens rather than as text, and the literals inside them.
pub use crate::decl::{
    annotation_named, annotation_string, annotation_value_text, annotations_of, has_modifier,
    modifier_words, named_child_of, node_text, simple_name, string_literal, type_declarations,
};

// The abbreviations a Java file expands, as a table the caller shapes for the wire.
pub use crate::templates::{matching as matching_templates, Template, TEMPLATES};

// Variable names: the one a written type reads as, and the one a declaration at a caret is about to
// be given (the ghost text after `private final OrderRepository `).
pub use crate::declaration_name::{
    declaration_name_at, declared_variable_names, DeclarationKind, DeclarationName, NameContext,
};
pub use crate::names::suggested_name_for_type;

// Postfix templates: the subject found by scanning, its shape read off its type, the expansions the
// module's language level allows.
pub use crate::postfix::{
    expansions as postfix_expansions, indent_unit as postfix_indent_unit, shape_of as postfix_shape,
    subject_start as postfix_subject_start, Element as PostfixElement, Expansion as PostfixExpansion,
    Length as PostfixLength, OptionalShape as PostfixOptional, PostfixContext, Stop as PostfixStop,
    Subject as PostfixSubject, ValueShape as PostfixShape, Written as PostfixWritten,
};
