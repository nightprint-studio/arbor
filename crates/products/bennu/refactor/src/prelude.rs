//! Canonical entry point for `bennu-refactor`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_refactor::prelude::...`. A host needs [`refactorings_at`] and [`plan_for`]; the
//! individual transforms are reachable for a caller that wants one.

// The one call the editor makes, and the one it makes again when a row is chosen.
pub use crate::offers::{plan_for, plans_at, refactorings_at};

// What comes back.
pub use crate::plan::{
    language_level, merge_throws, written_name, MemberTransfer, NeedsLevel, NewSource, Outcome,
    Plan, RefactorEdit, Refusal, SelectorGuard, ThrowsSlot, TypeGuard, TypeNeed, TypeSlot,
    SWITCHABLE,
};

// The individual transforms, for a caller that wants one rather than the list.
pub use crate::create::{create_method, missing_type_at, new_type_source, MissingType};
pub use crate::extract_method::extract_method;
pub use crate::extract_var::{extract_constant, extract_variable, TYPE_PLACEHOLDER};
pub use crate::declaration::{from_var, join_declaration, split_declaration, to_var};
pub use crate::if_statement::{invert_if, merge_nested_if};
pub use crate::inline_method::inline_method;
pub use crate::field::introduce_field;
pub use crate::inline_var::inline_variable;
pub use crate::move_class::move_class;
pub use crate::move_member::{
    adapt_modifiers, member_moves, move_member_plan, move_member_to, move_site, transfer_into,
    MoveDirection, MoveSite,
};
pub use crate::switch::if_chain_to_switch;

// Where a member goes, for a caller that has to write one into a file this crate never saw.
pub use crate::body::{append_point, body_of, member_indent, reindent, type_named};
