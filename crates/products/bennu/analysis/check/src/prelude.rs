//! Canonical entry point for `bennu-check`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_check::prelude::...`.

pub use crate::annotation::annotation_elements::annotation_element_errors_in;
pub use crate::annotation::annotations::{annotation_errors, duplicate_annotation_values_nodes};
pub use crate::calls::arguments::argument_type_errors;
pub use crate::calls::arity::arity_errors;
pub use crate::flow::branches::branch_errors_nodes;
pub use crate::calls::capture::capture_errors_nodes;
pub use crate::typing::casts::type_compat_errors;
pub use crate::engine::check::{
    check_file, check_file_resolved, FileContext, MAX_DIAGNOSTICS, MAX_MESSAGE_CHARS,
};
pub use crate::engine::check_id::CheckId;
pub use crate::engine::quarantine::{quarantine, Quarantine};

// The javac diagnostic catalog: every error javac can raise, and whether Bennu answers it. Lets a
// Bennu diagnostic name the javac error it stands for.
pub use crate::engine::javac::{coverage, javac_keys, missing, Coverage};
pub use crate::decls::constructors::super_constructor_errors;
pub use crate::decls::ctor_before::ctor_before_errors_nodes;
pub use crate::decls::declarations::declaration_errors;
pub use crate::decls::duplicates::duplicate_signatures;
pub use crate::calls::fields::unknown_fields;
pub use crate::calls::functional::functional_errors;
pub use crate::source::imports::{
    duplicate_imports, import_inventory, unresolved_imports, unresolved_static_imports,
    unused_imports, ImportEntry,
};
pub use crate::engine::incremental::{check_file_resolved_incremental, IncrementalCache};
pub use crate::hierarchy::inheritance::{inheritance_errors, missing_abstract_impls};
pub use crate::calls::lambdas::lambda_capture_errors;
pub use crate::decls::local_class::local_class_errors_nodes;
pub use crate::calls::members::unknown_members;
pub use crate::source::naming::{class_name_matches_file, type_file_mismatches, TypeFileMismatch};
pub use crate::decls::unused_member::RUNTIME_NAMES;
pub use crate::hierarchy::override_access::override_access_errors_in;
pub use crate::hierarchy::override_return::override_return_errors_in;
pub use crate::source::packaging::{change_package, change_package_edit, package_mismatch};
pub use crate::support::resolve::{inherited_member_type, same_package_binary, type_binary};
pub use crate::flow::returns::{missing_return, return_statement_errors};
pub use crate::decls::self_ref::self_ref_errors_nodes;
pub use crate::source::special_files::special_file_errors;
pub use crate::source::statements::invalid_statements;
pub use crate::hierarchy::static_via_instance::static_via_instance_warnings_in;
pub use crate::switching::switches::{switch_selector_errors, switch_yield_errors};
pub use crate::source::syntax::syntax_errors;
pub use crate::support::text::{excerpt, short, EXCERPT_CHARS};
pub use crate::typing::type_arg_arity::type_arg_arity_errors;
pub use crate::typing::types::unresolved_types;
pub use crate::calls::undefined_var::undefined_var;
pub use crate::calls::unresolved_call::unresolved_call;
pub use crate::source::var_target::var_target_errors_nodes;
pub use crate::source::version::version_errors;

// The wire diagnostic the checks emit, re-exported so a consumer reaches it through this prelude.
pub use bennu_proto::prelude::Diagnostic;

// The unhandled-checked-exception analysis's structured answer, for the quick-fix that repairs it:
// which exception, where a `throws` clause goes, and what a `try` would wrap. The diagnostic form
// is the same analysis with a sentence on it — see `checked_call::UnhandledCall`.
pub use crate::throwing::checked_call::{
    checked_exceptions_in, unhandled_calls_in, CheckedExceptions, UnhandledCall,
};

// The constants of an enum, as the switch checks identify them — the input to the quick-fix that
// fills in a non-exhaustive switch.
pub use crate::switching::enum_switch::enum_constants;

// Every named node of a parsed tree — the one traversal the resolver-backed checks share. Exposed
// because a consumer that wants to run one of them (a quick-fix recomputing its own diagnostic)
// needs the same slice they take.
pub use crate::engine::check::collect_nodes;

// The blank `final` fields the definite-assignment check reports, as name spans — the input to the
// fixes that initialise them, offered from the caret before validation has run.
pub use crate::decls::init_checks::uninitialized_final_fields;

// The project's inspection policy — which checks report, and how loudly. Severity per kind from the
// config, suppression from the source itself.
pub use crate::engine::inspections::{Inspections, Level as InspectionLevel};


// Data flow — null dereference, constant condition, dead store.
pub use crate::flow::dataflow::dataflow_errors_in;
pub use crate::throwing::dead_catch::dead_catch_errors_in;
