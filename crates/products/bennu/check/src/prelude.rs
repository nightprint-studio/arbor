//! Canonical entry point for `bennu-check`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_check::prelude::...`.

pub use crate::annotation_elements::annotation_element_errors_in;
pub use crate::annotations::{annotation_errors, duplicate_annotation_values_nodes};
pub use crate::arguments::argument_type_errors;
pub use crate::arity::arity_errors;
pub use crate::branches::branch_errors_nodes;
pub use crate::capture::capture_errors_nodes;
pub use crate::casts::type_compat_errors;
pub use crate::check::{
    check_file, check_file_resolved, FileContext, MAX_DIAGNOSTICS, MAX_MESSAGE_CHARS,
};
pub use crate::check_id::CheckId;

// The javac diagnostic catalog: every error javac can raise, and whether Bennu answers it. Lets a
// Bennu diagnostic name the javac error it stands for.
pub use crate::javac::{coverage, javac_keys, missing, Coverage};
pub use crate::constructors::super_constructor_errors;
pub use crate::ctor_before::ctor_before_errors_nodes;
pub use crate::declarations::declaration_errors;
pub use crate::duplicates::duplicate_signatures;
pub use crate::fields::unknown_fields;
pub use crate::functional::functional_errors;
pub use crate::imports::{
    duplicate_imports, import_inventory, unresolved_imports, unresolved_static_imports,
    unused_imports, ImportEntry,
};
pub use crate::incremental::{check_file_resolved_incremental, IncrementalCache};
pub use crate::inheritance::{inheritance_errors, missing_abstract_impls};
pub use crate::lambdas::lambda_capture_errors;
pub use crate::local_class::local_class_errors_nodes;
pub use crate::members::unknown_members;
pub use crate::naming::class_name_matches_file;
pub use crate::override_access::override_access_errors_in;
pub use crate::override_return::override_return_errors_in;
pub use crate::packaging::{change_package, change_package_edit, package_mismatch};
pub use crate::resolve::{inherited_member_type, same_package_binary, type_binary};
pub use crate::returns::{missing_return, return_statement_errors};
pub use crate::self_ref::self_ref_errors_nodes;
pub use crate::special_files::special_file_errors;
pub use crate::statements::invalid_statements;
pub use crate::static_via_instance::static_via_instance_warnings_in;
pub use crate::switches::{switch_selector_errors, switch_yield_errors};
pub use crate::syntax::syntax_errors;
pub use crate::text::{excerpt, short, EXCERPT_CHARS};
pub use crate::type_arg_arity::type_arg_arity_errors;
pub use crate::types::unresolved_types;
pub use crate::undefined_var::undefined_var;
pub use crate::var_target::var_target_errors_nodes;
pub use crate::version::version_errors;

// The wire diagnostic the checks emit, re-exported so a consumer reaches it through this prelude.
pub use bennu_proto::prelude::Diagnostic;

// The unhandled-checked-exception analysis's structured answer, for the quick-fix that repairs it:
// which exception, where a `throws` clause goes, and what a `try` would wrap. The diagnostic form
// is the same analysis with a sentence on it — see `checked_call::UnhandledCall`.
pub use crate::checked_call::{unhandled_calls_in, UnhandledCall};

// The constants of an enum, as the switch checks identify them — the input to the quick-fix that
// fills in a non-exhaustive switch.
pub use crate::enum_switch::enum_constants;

// Every named node of a parsed tree — the one traversal the resolver-backed checks share. Exposed
// because a consumer that wants to run one of them (a quick-fix recomputing its own diagnostic)
// needs the same slice they take.
pub use crate::check::collect_nodes;

// The project's inspection policy — which checks report, and how loudly. Severity per kind from the
// config, suppression from the source itself.
pub use crate::inspections::{Inspections, Level as InspectionLevel};


// Data flow — null dereference, constant condition, dead store.
pub use crate::dataflow::dataflow_errors_in;
pub use crate::dead_catch::dead_catch_errors_in;
