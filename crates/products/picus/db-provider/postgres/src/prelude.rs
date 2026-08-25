//! Canonical entry point for `picus-db-postgres`'s public API.
//!
//! Workspace convention: call sites (`picus-be`) reach this crate through
//! `picus_db_postgres::prelude::...`. In practice a call site only ever needs
//! [`PostgresProvider`] — everything else is reached through the
//! `picus_db_api` traits.

//! ## What is deliberately *not* here
//!
//! The capability modules — `activity`, `bind`, `depends`, `origins`, `plan`, `tx` —
//! and `catalog`. They are `pub` so rustdoc can navigate them, but no call site outside
//! this crate reaches them: they are what [`PgSession`] delegates to, and everyone
//! else arrives through the `DbSession` trait.
//!
//! Exporting them would also be actively harmful. A prelude is glob-imported, and
//! `begin`, `commit`, `rollback`, `state` and `explain` are exactly the names a
//! consumer is likely to have of its own — a prelude that quietly shadows them is
//! worse than one that is a few entries short.

pub use crate::cursor::{
    close_statement, count_statements, declare_cursor, explain_statement, plan_execution,
    plan_row_estimate, relation_query, window_statements, CursorHandle, CursorRegistry,
    ExecutionPlan, IDLE_TTL, MAX_OPEN,
};
pub use crate::provider::PostgresProvider;
pub use crate::session::PgSession;
pub use crate::sql::{quote_ident, quote_qualified, statement_kind, StatementKind};
