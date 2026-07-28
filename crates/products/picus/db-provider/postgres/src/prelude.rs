//! Canonical entry point for `picus-db-postgres`'s public API.
//!
//! Workspace convention: call sites (`picus-be`) reach this crate through
//! `picus_db_postgres::prelude::...`. In practice a call site only ever needs
//! [`PostgresProvider`] — everything else is reached through the
//! `picus_db_api` traits.

pub use crate::cursor::{
    close_statement, count_statements, declare_cursor, explain_statement, plan_execution,
    plan_row_estimate, relation_query, window_statements, CursorHandle, CursorRegistry,
    ExecutionPlan, IDLE_TTL, MAX_OPEN,
};
pub use crate::provider::PostgresProvider;
pub use crate::session::PgSession;
pub use crate::sql::{quote_ident, quote_qualified, statement_kind, StatementKind};
