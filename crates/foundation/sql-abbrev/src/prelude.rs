//! Canonical entry point for `arbor-sql-abbrev`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `arbor_sql_abbrev::prelude::...` (or one `use arbor_sql_abbrev::prelude::*;`).
//! The submodules stay public for rustdoc navigation, but they are not the path a
//! host should be importing from.
//!
//! A host needs, in practice, four names: [`SchemaView`] to describe the database,
//! [`expand`] to get a [`Statement`], [`context_at`] to drive completion, and
//! [`AbbrevError`] to show the refusal. [`render`] as well, if it has no emitter
//! of its own.

pub use crate::context::{context_at, CursorContext};
pub use crate::error::AbbrevError;
pub use crate::expand::{expand, MAX_ROWS};
pub use crate::join::{keys_between, JoinKey};
pub use crate::parse::parse;
pub use crate::render::{render, Case, RenderStyle};
pub use crate::schema::{ColumnMeta, ForeignKeyMeta, SchemaView, TableMeta, ValueKind};
pub use crate::span::{Slot, Span};
pub use crate::numbering::number;
pub use crate::statement::{
    Assignment, ColumnChange, ColumnRef, InsertRow, Join, JoinCondition, Operator, Predicate,
    Statement, TableRef, Value,
};
pub use crate::syntax::{ChangeItem, ChangeKind, Parsed, Verb};
