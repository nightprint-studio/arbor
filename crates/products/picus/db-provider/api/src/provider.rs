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
use crate::query::{ExecuteResult, ResultCount, ResultWindow};
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
///
/// ## Held results — the contract an implementation must keep
///
/// [`execute`](Self::execute) and [`open_relation`](Self::open_relation) may leave a
/// **held result** behind, named by [`ExecuteResult::result_id`]. Whatever the
/// engine's mechanism (a scrollable cursor, a materialised temp relation), four
/// rules bind every implementation, because the thing being held sits on somebody's
/// production server:
///
/// 1. **Several results coexist per session.** Query tabs share one connection;
///    a result is named per result, never per session, and opening one must not
///    disturb another.
/// 2. **A result is closed by [`close_result`](Self::close_result)**, which is
///    *idempotent*: closing one that is already gone is success. The caller closing
///    twice, or closing something the engine already reclaimed, is normal.
/// 3. **The implementation bounds what the caller forgets.** A caller that
///    crashes, or a tab closed by killing the window, must not leave a result held
///    forever: an implementation caps how many it keeps per session and reclaims
///    the ones nobody has touched for a stated period. That policy is documented in
///    the engine crate, not guessed at here.
/// 4. **Closing the session closes its results.**
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

    /// Run a statement — **any** statement. The one door.
    ///
    /// A read opens a held result and returns its first `window` rows; a write
    /// returns what it changed and holds nothing. Deciding which is the engine's
    /// job, not the caller's: classifying SQL correctly needs a dialect-aware scan,
    /// and a caller that gets it wrong gets it wrong silently.
    ///
    /// On a read-only connection a non-read statement must be refused with
    /// [`DbError::ReadOnly`](crate::error::DbError::ReadOnly) — and the refusal has
    /// to be real, not a hidden button.
    async fn execute(&self, sql: &str, window: u32) -> DbResult<ExecuteResult>;

    /// Open a relation's data, as [`execute`](Self::execute) opens a read.
    ///
    /// The relation name arrives unquoted and possibly schema-qualified; turning it
    /// into SQL is the engine's job, because quoting rules are engine-specific and a
    /// caller composing them would be writing one dialect's syntax for all of them.
    async fn open_relation(&self, relation: &str, window: u32) -> DbResult<ExecuteResult>;

    /// One window over a held result. Any offset, forwards or backwards: the grid
    /// is virtualised and the user may jump to the end.
    ///
    /// An offset past the end is not an error — it is an empty window whose
    /// `end_of_result` is true.
    async fn result_window(&self, result_id: &str, offset: u64, limit: u32)
        -> DbResult<ResultWindow>;

    /// The exact number of rows in a held result.
    ///
    /// Potentially long-running, and interruptible by [`cancel`](Self::cancel) like
    /// any other statement — a count over a large result is precisely the thing a
    /// user changes their mind about.
    async fn count_result(&self, result_id: &str) -> DbResult<ResultCount>;

    /// Release a held result. Idempotent: an unknown id is success.
    async fn close_result(&self, result_id: &str) -> DbResult<()>;

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

    /// Close the session, and with it every result it still holds. Idempotent.
    async fn close(&self) -> DbResult<()>;
}
