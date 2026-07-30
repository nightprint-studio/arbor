//! Canonical entry point for `picus-emit`'s public API.

pub use crate::emit_for_target;
pub use crate::literal::{
    ident, is_numeric_type, literal, now_function, read, validate_value, Written,
};
pub use crate::statement::{
    insert_rows, plain_statement, predicate_sql, statement_for, update_row, PORTABLE_UPSERT,
};
