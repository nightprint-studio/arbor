//! Canonical entry point for `bennu-refactor`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_refactor::prelude::...`. A host needs [`refactorings_at`] and [`plan_for`]; the
//! individual transforms are reachable for a caller that wants one.

// The one call the editor makes, and the one it makes again when a row is chosen.
pub use crate::offers::{plan_for, plans_at, refactorings_at};

// What comes back.
pub use crate::plan::{
    merge_throws, written_name, Outcome, Plan, RefactorEdit, Refusal, ThrowsSlot, TypeNeed,
    TypeSlot,
};

// The individual transforms, for a caller that wants one rather than the list.
pub use crate::create::{create_method, missing_type_at, new_type_source, MissingType};
pub use crate::extract_method::extract_method;
pub use crate::extract_var::{extract_constant, extract_variable, TYPE_PLACEHOLDER};
pub use crate::inline_method::inline_method;
pub use crate::inline_var::inline_variable;
