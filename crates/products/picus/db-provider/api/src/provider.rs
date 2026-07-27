//! The two traits every engine implements: [`DbProvider`] (what an engine *is*)
//! and [`DbSession`] (what one live connection *does*).
//!
//! The split matters. A provider is stateless, always present, and can answer for
//! its engine without a server anywhere — which is how Oracle can be a first-class
//! engine for scripts while having no driver. A session only exists once someone
//! successfully connected.
//!
//! `async_trait` is used so these stay dyn-compatible (`Arc<dyn DbProvider>`);
//! plain `async fn` in traits is not object-safe yet.

use async_trait::async_trait;

use crate::connection::{ConnectionSpec, ConnectionStatus};
use crate::descriptor::DbProviderDescriptor;
use crate::error::DbResult;
use crate::kind::EngineKind;
use crate::query::{QueryResult, RowPage};
use crate::schema::{SchemaSnapshot, TableInfo};
use crate::secret::Secret;

/// One database engine.
#[async_trait]
pub trait DbProvider: Send + Sync {
    /// Which engine this is.
    fn kind(&self) -> EngineKind;

    /// The document the UI renders from — connection fields, capabilities,
    /// emission traits, labels. Cheap: callers may ask per keystroke.
    fn descriptor(&self) -> DbProviderDescriptor;

    /// Open a session.
    ///
    /// `secret` is `None` when no password is stored, which is legitimate (trust
    /// or peer authentication). A provider that requires one returns
    /// [`DbError::SecretMissing`](crate::error::DbError::SecretMissing) rather than
    /// a generic connect failure, so the UI can ask for the password instead of
    /// showing a network error.
    ///
    /// A provider whose engine has no driver returns
    /// [`DbError::NoDriver`](crate::error::DbError::NoDriver) — a distinct, honest
    /// message, never a fake failure.
    async fn connect(
        &self,
        spec: &ConnectionSpec,
        secret: Option<Secret>,
    ) -> DbResult<Box<dyn DbSession>>;
}

/// One live connection.
///
/// Every method takes `&self`: a session is shared behind an `Arc` and must be
/// usable from the handler serving a query and from the one serving a cancel at
/// the same time — that is the entire point of being able to cancel.
#[async_trait]
pub trait DbSession: Send + Sync {
    /// Liveness + the server banner.
    async fn status(&self) -> ConnectionStatus;

    /// Read the whole schema: tables, views, sequences, triggers, with columns.
    ///
    /// Constraints and indexes are deliberately **not** included here — a schema
    /// with hundreds of tables would pay for detail nobody has opened yet. Ask
    /// [`table_detail`](Self::table_detail) when a tab actually opens.
    async fn read_schema(&self) -> DbResult<SchemaSnapshot>;

    /// The full detail of one relation: columns, primary key, foreign keys and
    /// indexes.
    async fn table_detail(&self, name: &str) -> DbResult<TableInfo>;

    /// One page of a relation's rows.
    async fn fetch_page(&self, name: &str, offset: u64, limit: u32) -> DbResult<RowPage>;

    /// Run a statement.
    ///
    /// `limit` caps the rows returned; the result reports whether it truncated.
    /// On a read-only connection a non-read statement must be refused with
    /// [`DbError::ReadOnly`](crate::error::DbError::ReadOnly) — and the refusal has
    /// to be real, not a hidden button.
    async fn execute(&self, sql: &str, limit: u32) -> DbResult<QueryResult>;

    /// Ask the server to cancel whatever this session is running. A no-op when
    /// nothing is running; errors only when the request itself could not be sent.
    async fn cancel(&self) -> DbResult<()>;

    /// Read the application version from the project's version table.
    ///
    /// The table, the column and the optional filter are configuration, not
    /// constants — plenty of projects name them differently, and some stamp no
    /// date at all. Returns `None` when the table isn't there, which is normal for
    /// a database that isn't this project's.
    async fn read_db_version(
        &self,
        table: &str,
        column: &str,
        filter: &str,
    ) -> DbResult<Option<String>>;

    /// Close the session. Idempotent.
    async fn close(&self) -> DbResult<()>;
}
