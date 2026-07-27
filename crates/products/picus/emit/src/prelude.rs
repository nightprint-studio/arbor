//! Canonical entry point for `picus-emit`'s public API.

pub use crate::emit_for_target;
pub use crate::literal::{ident, literal, looks_like_expression, now_function, validate_value};
pub use crate::statement::plain_statement;
