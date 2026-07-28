//! [`PgSession`] — one live PostgreSQL connection.
//!
//! Reading and writing are one door ([`DbSession::execute`]): a read opens a held
//! result the grid scrolls through, a write returns what it changed. Which of the
//! two a statement is, is decided here rather than by the caller — see
//! [`crate::cursor`] for why the result is a `WITH HOLD` cursor and what that costs.
//!
//! How a value becomes a cell lives in [`crate::rows`]; the SQL a cursor is made of
//! lives in [`crate::cursor::sql`]. What is left here is the part that needs a
//! connection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use async_trait::async_trait;
use picus_db_api::prelude::*;
use tokio_postgres::types::Type;
use tokio_postgres::{Client, SimpleQueryMessage};

use crate::catalog;
use crate::cursor::{self, CursorHandle, CursorRegistry, ExecutionPlan};
use crate::error::map_pg;
use crate::rows::{self, Fetched};
use crate::sql::{guard_read_only, quote_ident, quote_qualified};
use crate::tls::TlsChoice;

/// A live session. Shared behind an `Arc`: the handler serving a query and the one
/// serving its cancellation run concurrently, which is the entire point of being
/// able to cancel.
pub struct PgSession {
    client: Client,
    /// Cancellation key issued by the server at connect time. Cancelling opens a
    /// *second*, short-lived connection — which is why it works while the first one
    /// is busy.
    cancel_token: tokio_postgres::CancelToken,
    tls: TlsChoice,
    spec: ConnectionSpec,
    server_version: String,
    /// Set when the connection is torn down, so `status()` stops claiming health.
    closed: Mutex<bool>,
    /// Ordinal of the statement currently being executed (0 = none yet).
    run_seq: AtomicU64,
    /// The highest ordinal a cancellation has covered.
    ///
    /// The server's cancel key only interrupts what is running **at the instant it
    /// arrives**, and `execute` is more than one round trip: a Cancel landing in a
    /// gap between them would hit nothing and be lost, and the statement would then
    /// run in full with the user watching a button that did nothing. These two
    /// counters remember the request instead. Scoping it to an ordinal (rather than
    /// a bare flag) is what stops a cancel arriving after a query already finished
    /// from killing the *next* one.
    cancelled_seq: AtomicU64,
    /// The results this session is holding open, and the policy that ends them.
    cursors: CursorRegistry,
}

impl PgSession {
    pub(crate) fn new(
        client: Client,
        tls: TlsChoice,
        spec: ConnectionSpec,
        server_version: String,
    ) -> Self {
        let cancel_token = client.cancel_token();
        Self {
            client,
            cancel_token,
            tls,
            spec,
            server_version,
            closed: Mutex::new(false),
            run_seq: AtomicU64::new(0),
            cancelled_seq: AtomicU64::new(0),
            cursors: CursorRegistry::new(),
        }
    }

    /// Claim the next statement ordinal.
    fn begin_run(&self) -> u64 {
        self.run_seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Fail when a cancellation has covered this run.
    fn check_cancelled(&self, seq: u64) -> DbResult<()> {
        if self.cancelled_seq.load(Ordering::SeqCst) >= seq {
            return Err(DbError::Cancelled);
        }
        Ok(())
    }

    /// The schema this session browses: the configured one, or PostgreSQL's default.
    fn schema(&self) -> &str {
        if self.spec.schema.is_empty() {
            "public"
        } else {
            &self.spec.schema
        }
    }

    fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Column types for a statement, when it is preparable. `None` is not a
    /// failure — it means "show the columns untyped", which several perfectly good
    /// statements (`SET`, a multi-statement paste) legitimately produce.
    async fn column_types(&self, sql: &str) -> Option<Vec<(String, Type)>> {
        let stmt = self.client.prepare(sql).await.ok()?;
        Some(stmt.columns().iter().map(|c| (c.name().to_string(), c.type_().clone())).collect())
    }

    /// Run a statement over the simple protocol and shape the reply.
    async fn run_simple(
        &self,
        sql: &str,
        types: Option<&[(String, Type)]>,
        limit: u32,
    ) -> DbResult<Fetched> {
        let messages = self.client.simple_query(sql).await.map_err(map_pg)?;
        Ok(rows::collect(messages, types, limit))
    }
}

// ── Held results ───────────────────────────────────────────────────────────────

impl PgSession {
    /// Release the results nobody has touched for the idle period.
    ///
    /// Called at the start of every statement, which is what makes the policy hold
    /// in the case it exists for: a user still working on this connection with one
    /// abandoned tab. A connection nobody touches again keeps its results until it
    /// is closed or the backend exits — swept lazily like this rather than by a
    /// timer, because a background thread issuing SQL on a connection somebody else
    /// is using is a far worse hazard than a tuplestore living until disconnect.
    async fn sweep_idle(&self) {
        for name in self.cursors.expired(Instant::now()) {
            self.close_cursor(&name).await;
        }
    }

    /// `CLOSE` one cursor, best-effort. A cursor the server has already discarded
    /// (session reset, an error that aborted it) is not a problem worth surfacing:
    /// the caller's intent was for it to be gone.
    async fn close_cursor(&self, name: &str) {
        let _ = self.client.simple_query(&cursor::close_statement(name)).await;
    }

    /// The planner's row estimate for a query — the length the scrollbar starts
    /// with, before anybody has counted anything.
    ///
    /// Best-effort throughout: an `EXPLAIN` that fails costs an unknown total, not a
    /// failed read. `None` must render as "unknown" and never as zero.
    async fn estimated_rows(&self, body: &str) -> Option<i64> {
        let messages = self.client.simple_query(&cursor::explain_statement(body)).await.ok()?;
        let first_line = messages.iter().find_map(|m| match m {
            SimpleQueryMessage::Row(r) => r.get(0).map(str::to_string),
            _ => None,
        })?;
        cursor::plan_row_estimate(&first_line)
    }

    /// Declare a held cursor over `body` and return its first window.
    async fn open_cursor(
        &self,
        body: &str,
        window: u32,
        seq: u64,
        started: Instant,
    ) -> DbResult<ExecuteResult> {
        let estimated_rows = self.estimated_rows(body).await;
        let types = self.column_types(body).await;

        // Those are round trips of their own; a Cancel that landed during one has
        // nothing to interrupt on the server, so it is honoured here.
        self.check_cancelled(seq)?;

        let id = self.cursors.next_id();
        // THE expensive statement. `WITH HOLD` means the server runs the query to
        // completion and stores the whole result before this returns — see
        // `crate::cursor` for why that price is the one worth paying. Everything
        // after it is cheap, and everything after it must clean up if it fails.
        self.client.simple_query(&cursor::declare_cursor(&id, body)).await.map_err(map_pg)?;

        let handle = CursorHandle { name: id.clone(), types: types.clone() };
        for evicted in self.cursors.register(&id, handle, Instant::now()) {
            self.close_cursor(&evicted).await;
        }

        // A cancel that arrived while the DECLARE was running is honoured here: the
        // statement completed, but the user asked for it not to, and the cursor it
        // left behind has to go with it.
        let first = match self.read_window(&id, types.as_deref(), 0, window).await {
            Ok(read) => self.check_cancelled(seq).map(|()| read),
            Err(e) => Err(e),
        };

        match first {
            Ok((fetched, end_of_result)) => {
                Ok(ExecuteResult {
                    result_id: Some(id),
                    columns: Some(fetched.columns),
                    row_count: fetched.rows.len(),
                    rows: fetched.rows,
                    estimated_rows,
                    // The exact figure is asked for separately: a `count` here would
                    // put a full walk of the result in front of the first row.
                    total_rows: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    end_of_result,
                    affected: None,
                })
            }
            Err(e) => {
                // A declared cursor whose first window failed is exactly the leak
                // this whole module exists to prevent. Deregistered *before* the
                // `CLOSE`, so a sweep running alongside cannot decide to close the
                // same one twice.
                if let Some(name) = self.cursors.remove(&id) {
                    self.close_cursor(&name).await;
                }
                Err(e)
            }
        }
    }

    /// Position and read one window, asking for one row more than is wanted.
    ///
    /// That extra row is evidence, not data: it is what turns "this window is full"
    /// into the fact "there is more after it", instead of a guess that calls every
    /// exactly-full result unfinished. It is never returned.
    async fn read_window(
        &self,
        name: &str,
        types: Option<&[(String, Type)]>,
        offset: u64,
        limit: u32,
    ) -> DbResult<(Fetched, bool)> {
        let limit = limit.max(1);
        let probe = limit.saturating_add(1);
        let sql = cursor::window_statements(name, offset, probe);
        let mut fetched = self.run_simple(&sql, types, probe).await?;

        let end_of_result = fetched.rows.len() as u32 <= limit;
        fetched.rows.truncate(limit as usize);
        Ok((fetched, end_of_result))
    }

    /// Run a statement as the user wrote it, holding nothing.
    async fn run_direct(
        &self,
        sql: &str,
        window: u32,
        seq: u64,
        started: Instant,
    ) -> DbResult<ExecuteResult> {
        let types = self.column_types(sql).await;
        self.check_cancelled(seq)?;

        let window = window.max(1);
        let probe = window.saturating_add(1);
        let mut fetched = self.run_simple(sql, types.as_deref(), probe).await?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        if !fetched.had_result_set {
            return Ok(ExecuteResult {
                result_id: None,
                columns: None,
                rows: Vec::new(),
                estimated_rows: None,
                total_rows: None,
                elapsed_ms,
                row_count: 0,
                end_of_result: true,
                affected: fetched.last_command_count,
            });
        }

        // A read that could not be cursored still has to say when it was cut short.
        // `resultId: null` with `endOfResult: false` is the honest statement of
        // "there was more and there is no way to ask for it" — the state a
        // multi-statement paste of a large SELECT lands in.
        let end_of_result = fetched.rows.len() as u32 <= window;
        fetched.rows.truncate(window as usize);

        Ok(ExecuteResult {
            result_id: None,
            columns: Some(fetched.columns),
            row_count: fetched.rows.len(),
            rows: fetched.rows,
            estimated_rows: None,
            total_rows: None,
            elapsed_ms,
            end_of_result,
            affected: None,
        })
    }
}

#[async_trait]
impl DbSession for PgSession {
    async fn status(&self) -> ConnectionStatus {
        let state = if self.is_closed() || self.client.is_closed() {
            ConnectionState::Disconnected
        } else if self.spec.read_only {
            ConnectionState::ReadOnly
        } else {
            ConnectionState::Connected
        };
        ConnectionStatus {
            id: self.spec.id.clone(),
            state,
            server_version: self.server_version.clone(),
            db_version: String::new(),
            message: String::new(),
        }
    }

    async fn read_schema(&self) -> DbResult<SchemaSnapshot> {
        let schema = self.schema();
        let (tables, views) = catalog::read_relations(&self.client, schema).await?;
        let sequences = catalog::read_sequences(&self.client, schema).await?;
        let triggers = catalog::read_triggers(&self.client, schema).await?;
        Ok(SchemaSnapshot { tables, views, sequences, triggers })
    }

    async fn table_detail(&self, name: &str) -> DbResult<TableInfo> {
        let schema = self.schema();
        // Pinned to this relation: reading the whole catalogue to find one of
        // several hundred is what made opening a tab slow.
        let mut info = catalog::read_relation(&self.client, schema, name)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("relation {name}")))?;

        match info.kind {
            RelationKind::View => {
                info.definition = catalog::read_view_definition(&self.client, schema, name).await?;
            }
            RelationKind::Table => {
                info.primary_key_name =
                    catalog::read_primary_key_name(&self.client, schema, name).await?;
                info.foreign_keys =
                    Some(catalog::read_foreign_keys(&self.client, schema, name).await?);
                info.indexes = Some(catalog::read_indexes(&self.client, schema, name).await?);
            }
        }
        Ok(info)
    }

    async fn execute(&self, sql: &str, window: u32) -> DbResult<ExecuteResult> {
        // Courtesy check first — a clear product message without a round-trip. The
        // server is still the authority: the session runs in a read-only
        // transaction mode, so anything this misses is refused there.
        guard_read_only(sql, self.spec.read_only)?;

        let seq = self.begin_run();
        let started = Instant::now();
        self.sweep_idle().await;

        match cursor::plan_execution(sql) {
            ExecutionPlan::Cursor(body) => self.open_cursor(body, window, seq, started).await,
            ExecutionPlan::Direct => self.run_direct(sql, window, seq, started).await,
        }
    }

    async fn open_relation(&self, relation: &str, window: u32) -> DbResult<ExecuteResult> {
        // Deliberately the *same* path as a typed query, not a parallel one: a
        // relation tab and a query tab must scroll identically, and the only thing
        // that differs is who wrote the SELECT. The quoting is the engine's, which
        // is the whole reason this is a method rather than a string the caller
        // composes.
        let sql = cursor::relation_query(self.schema(), relation);
        self.execute(&sql, window).await
    }

    async fn result_window(
        &self,
        result_id: &str,
        offset: u64,
        limit: u32,
    ) -> DbResult<ResultWindow> {
        self.sweep_idle().await;
        let handle = self.cursors.touch(result_id, Instant::now()).ok_or_else(unknown_result)?;
        // Note the stored name, never `result_id`: nothing that arrived over the
        // wire is allowed to become an identifier in a statement.
        let (fetched, end_of_result) =
            self.read_window(&handle.name, handle.types.as_deref(), offset, limit).await?;
        Ok(ResultWindow { offset, rows: fetched.rows, end_of_result })
    }

    async fn count_result(&self, result_id: &str) -> DbResult<ResultCount> {
        self.sweep_idle().await;
        let handle = self.cursors.touch(result_id, Instant::now()).ok_or_else(unknown_result)?;

        // Counting takes a run ordinal like any other statement, which is what makes
        // Cancel work on it: `picus_cancel` sends the server's cancel key *and*
        // records the ordinal, so a cancel landing between the rewind and the walk
        // is honoured rather than lost.
        let seq = self.begin_run();
        let fetched = self
            .run_simple(&cursor::count_statements(&handle.name), None, 0)
            .await?;
        self.check_cancelled(seq)?;

        // `MOVE FORWARD ALL` reports how far it moved — the exact row count of this
        // very result, not of a re-execution that might disagree with it.
        Ok(ResultCount { total: fetched.last_command_count.unwrap_or(0) as i64 })
    }

    async fn close_result(&self, result_id: &str) -> DbResult<()> {
        if let Some(name) = self.cursors.remove(result_id) {
            self.close_cursor(&name).await;
        }
        // Closing something already gone is success: the caller wanted it gone.
        Ok(())
    }

    async fn cancel(&self) -> DbResult<()> {
        // Record the request before sending it: between a statement's round trips
        // there are instants where the server has nothing to cancel, and without
        // this the click would simply be lost.
        self.cancelled_seq.store(self.run_seq.load(Ordering::SeqCst), Ordering::SeqCst);
        match &self.tls {
            TlsChoice::Plain(connector) => {
                self.cancel_token.cancel_query(connector.clone()).await.map_err(map_pg)
            }
            TlsChoice::Rustls(connector) => {
                self.cancel_token.cancel_query(connector.clone()).await.map_err(map_pg)
            }
        }
    }

    async fn read_db_version(
        &self,
        table: &str,
        column: &str,
        filter: &str,
    ) -> DbResult<Option<String>> {
        if table.is_empty() || column.is_empty() {
            return Ok(None);
        }
        let where_clause =
            if filter.trim().is_empty() { String::new() } else { format!(" WHERE {filter}") };
        let sql = format!(
            "SELECT {}::text FROM {}{where_clause} LIMIT 1",
            quote_ident(column),
            quote_qualified(table)
        );

        match self.client.simple_query(&sql).await {
            Ok(messages) => Ok(messages.into_iter().find_map(|m| match m {
                SimpleQueryMessage::Row(r) => r.get(0).map(|s| s.to_string()),
                _ => None,
            })),
            // A database that is not this project's simply has no version table.
            // That is an ordinary state, not an error to shout about.
            Err(e) => match map_pg(e) {
                DbError::NotFound(_) => Ok(None),
                other => Err(other),
            },
        }
    }

    async fn close(&self) -> DbResult<()> {
        // Flagged first so `status()` stops claiming health while the closes run —
        // and in its own scope, because a lock guard must never cross an `await`.
        {
            *self.closed.lock().unwrap_or_else(|p| p.into_inner()) = true;
        }
        // The connection dying would release these anyway; doing it explicitly means
        // the server reclaims the storage now rather than whenever this process's
        // socket is finally noticed.
        for name in self.cursors.drain() {
            self.close_cursor(&name).await;
        }
        Ok(())
    }
}

/// The error for a window or a count asked of a result that is no longer held.
///
/// Written for the person reading it: the causes are all mundane (the tab was
/// closed, the result expired, the connection was reopened) and the remedy is
/// always the same one.
fn unknown_result() -> DbError {
    DbError::NotFound(
        "this result is no longer open — run the statement again to scroll it".to_string(),
    )
}
