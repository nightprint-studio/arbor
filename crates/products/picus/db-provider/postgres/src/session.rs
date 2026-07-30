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

/// The statement as it will be read, which of its columns were left out of it, and
/// the types the reply should be mapped with.
///
/// ## Why this is driven by the server's description and not by the SQL
///
/// The columns come from the `prepare` that already happens to type them, so this
/// knows a result carries a `bytea` whatever shape the statement had — a join, a
/// union, a CTE, a column named explicitly. An earlier version recognised only
/// `SELECT * FROM <one table>` and asked the catalogue about it, which meant
/// `SELECT allegato FROM archivio` still dragged every byte across.
///
/// ## Nothing is masked that cannot be described
///
/// Two refusals, and both are about the wrapper being able to name a column: a
/// result with a **duplicate** column name would make the reference ambiguous, and
/// one with an empty name cannot be referenced at all. Either way the statement is
/// left alone rather than rewritten into something that fails.
fn mask_large_objects(
    body: &str,
    types: Option<Vec<(String, Type)>>,
) -> (String, Vec<String>, Option<Vec<(String, Type)>>) {
    let plain = || (body.to_string(), Vec::new(), types.clone());
    let Some(described) = &types else { return plain() };

    let heavy: Vec<String> = described
        .iter()
        .filter(|(_, ty)| cursor::is_large_object(ty.name()))
        .map(|(name, _)| name.clone())
        .collect();
    if heavy.is_empty() {
        return plain();
    }

    let mut seen = std::collections::HashSet::new();
    if described.iter().any(|(name, _)| name.is_empty() || !seen.insert(name.clone())) {
        return plain();
    }

    let columns: Vec<(String, bool)> =
        described.iter().map(|(name, _)| (name.clone(), heavy.contains(name))).collect();
    // The masked columns come back as sizes, so the reply has to be mapped as
    // numbers — otherwise the grid right-aligns everything else and leaves the one
    // column that is now a number sitting on the left.
    let retyped: Vec<(String, Type)> = described
        .iter()
        .map(|(name, ty)| {
            let ty = if heavy.contains(name) { Type::INT4 } else { ty.clone() };
            (name.clone(), ty)
        })
        .collect();

    (cursor::masked_source(body, &columns), heavy, Some(retyped))
}

/// How long a session may spend releasing what it holds before it is closed anyway.
///
/// Short on purpose: this runs on the path out of trouble, and a user waiting to be
/// let out of a wedged connection is not in the mood to wait for tidiness.
const CLOSING_LIMIT: std::time::Duration = std::time::Duration::from_secs(3);

/// How long one catalogue query may take before the studio gives up on it.
///
/// Generous — a large catalogue on a loaded server is legitimately slow, and this
/// must never fire on a schema that is merely big.
const CATALOGUE_LIMIT: std::time::Duration = std::time::Duration::from_secs(60);

/// A catalogue read that cannot take forever.
///
/// **Only** the reads Picus issues on its own. A statement the user typed is never
/// bounded — a report that takes ten minutes is a report, and a tool that killed it
/// would be worse than one that waits — but the schema read is different in kind:
/// it is fired automatically the instant a connection opens, the interface has
/// nothing to show until it lands, and there is no button to cancel it. Left
/// unbounded it is the one call in the product that can look exactly like the
/// application having stopped, which is what it did.
///
/// The query keeps running on the server; what ends here is the waiting.
async fn bounded<T>(
    what: &'static str,
    read: impl std::future::Future<Output = DbResult<T>>,
) -> DbResult<T> {
    match tokio::time::timeout(CATALOGUE_LIMIT, read).await {
        Ok(result) => result,
        Err(_) => Err(DbError::Internal(format!(
            "reading the {what} of this schema took more than {}s, so Picus stopped waiting. \
             The connection is open — a very large catalogue, or a server under load, can do \
             this; the object tree will be empty until it is re-read.",
            CATALOGUE_LIMIT.as_secs()
        ))),
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
        for handle in self.cursors.expired(Instant::now()) {
            if handle.declared {
                self.close_cursor(&handle.name).await;
            }
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

    /// Read the first window of a query, and register a result the rest of it can
    /// be asked for through.
    ///
    /// ## Nothing is materialised to answer this
    ///
    /// The obvious implementation — declare the `WITH HOLD` cursor here — is the
    /// one that made a table of scanned documents take minutes to show five
    /// hundred rows, because `WITH HOLD` runs the query to completion and copies
    /// **every** row into a tuplestore before it answers. The window the caller
    /// asked for did not bound that in any way, which is the part that surprises
    /// people: a row limit on the grid is not a row limit on the server.
    ///
    /// So the first window streams through a cursor without `HOLD`, and the
    /// holdable one is declared only if somebody scrolls past it
    /// ([`declared`](CursorHandle::declared)). Most results never do — and the ones
    /// that do have a user watching rows on screen while it happens, rather than an
    /// empty panel.
    ///
    /// ## The transaction must always be ended
    ///
    /// The streaming read is `BEGIN … COMMIT` in one string. If it fails part-way —
    /// a statement the user got wrong, a projection the wrapper could not name —
    /// PostgreSQL leaves the session **inside an aborted transaction block**, and
    /// every later statement on that connection is refused until something ends it.
    /// Nothing else on this connection would, so the failure path rolls back before
    /// it does anything else. Without that, one bad query poisons the connection
    /// until the user reconnects, which is indistinguishable from the application
    /// having stopped working.
    ///
    /// ## What that costs, stated plainly
    ///
    /// The first window and the cursor are **two executions**. For a statement with
    /// no `ORDER BY`, PostgreSQL is free to return rows in a different sequence the
    /// second time, so a row shown in the first window may appear again — or not at
    /// all — once scrolling crosses into the cursor's snapshot. Within the cursor
    /// everything is stable, as before.
    ///
    /// That is a real weakening of the old guarantee and it is taken deliberately:
    /// the guarantee only held for people who waited out the materialisation, and
    /// on a large table nobody did.
    async fn open_result(
        &self,
        body: &str,
        window: u32,
        seq: u64,
        started: Instant,
    ) -> DbResult<ExecuteResult> {
        // The scrollbar's length, and the column types, before any rows. Both are
        // planning-only round trips; a Cancel that landed during one has nothing to
        // interrupt on the server, so it is honoured here.
        let estimated_rows = self.estimated_rows(body).await;
        let types = self.column_types(body).await;
        self.check_cancelled(seq)?;

        let window = window.max(1);
        let probe = window.saturating_add(1);
        // The statement as it will actually be read: the user's own, unless it
        // carries large objects, in which case they stand for themselves.
        // Ordering wins over masking. Masking means wrapping the statement in a
        // subquery, and PostgreSQL does not have to hand a sub-select's rows on in
        // the order it produced them — a parallel plan uses `Gather` rather than
        // `Gather Merge` and interleaves them. A grid in the wrong order is a wrong
        // answer; an ordered read that carries its large objects is a slow one, and
        // the bound below is what keeps it merely slow.
        let (source, masked_columns, types) = match cursor::orders_its_own_rows(body) {
            true => (body.to_string(), Vec::new(), types),
            false => mask_large_objects(body, types),
        };

        // The bound goes in the STATEMENT, not only in the `FETCH`. See
        // `cursor::bounded_body`: without it an `ORDER BY` sorts the whole table to
        // disk before returning a row, which is what made an ordered query look like
        // it had hung while another client answered it in seconds.
        //
        // It bounds THIS read and nothing else. `source` — unbounded — is what the
        // handle keeps, because the scrollable cursor and the exact count are about
        // the whole result, and a `LIMIT 501` baked into either would quietly turn
        // every result into five hundred rows.
        let first_read = cursor::bounded_body(&source, probe).unwrap_or_else(|| source.clone());

        // A generated name, never a fixed one: two of these can be in flight on one
        // connection (a background count on one tab, a Run on another), and a
        // `DECLARE` onto a name that already exists fails — which, before the
        // rollback below, would then poison the connection.
        let streaming = self.cursors.next_id();
        let first = cursor::first_window_statements(&streaming, &first_read, probe);

        let mut fetched = match self.run_simple(&first, types.as_deref(), probe).await {
            Ok(fetched) => fetched,
            Err(e) => {
                // THE line that keeps one bad query from taking the connection with
                // it. See the note above: without this the session sits in an
                // aborted transaction block and refuses everything afterwards.
                let _ = self.client.simple_query("ROLLBACK").await;
                // A cancellation is not a failure of the strategy, it is the user
                // saying stop — and the fallback below RE-RUNS the statement, as the
                // slow held-cursor read. Pressing Cancel would start the very query
                // that was being escaped from, and the tab would sit there having
                // already reported the cancel. Checked here rather than only after
                // the match, which the fallback's early return never reaches.
                self.check_cancelled(seq)?;
                // On stderr rather than swallowed: this path is a fallback, so the
                // user sees whatever the second attempt says, and the reason the
                // first one was abandoned would otherwise be lost entirely.
                eprintln!("picus: the streamed first window failed ({e}); using a held cursor");
                // A statement the user got wrong lands here, and so does one the
                // masking wrapper could not be applied to. The cursor path runs the
                // body as they wrote it, so its error quotes their SQL rather than
                // Picus's rewrite of it.
                return self.open_cursor(body, window, seq, started).await;
            }
        };
        self.check_cancelled(seq)?;

        let end_of_result = fetched.rows.len() as u32 <= window;
        fetched.rows.truncate(window as usize);

        let id = self.cursors.next_id();
        let handle = CursorHandle {
            name: id.clone(),
            types,
            // The *effective* statement, not the one that was typed: a later window
            // has to come back shaped the way the first one was, or the grid would
            // suddenly start showing megabytes of hex in a column of sizes.
            body: source,
            declared: false,
        };
        for evicted in self.cursors.register(&id, handle, Instant::now()) {
            if evicted.declared {
                self.close_cursor(&evicted.name).await;
            }
        }

        Ok(ExecuteResult {
            result_id: Some(id),
            columns: Some(fetched.columns),
            row_count: fetched.rows.len(),
            rows: fetched.rows,
            estimated_rows,
            // The exact figure is asked for separately: a `count` here would put a
            // full walk of the result in front of the first row.
            total_rows: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
            end_of_result,
            affected: None,
            masked_columns,
        })
    }

    /// Declare the `WITH HOLD` cursor over `body`, and return the handle to serve
    /// windows with.
    ///
    /// THE expensive statement, and now the one that only runs when somebody has
    /// asked for a row the first window did not contain.
    async fn declare_now(&self, id: &str, handle: CursorHandle) -> DbResult<CursorHandle> {
        self.client
            .simple_query(&cursor::declare_cursor(&handle.name, &handle.body))
            .await
            .map_err(map_pg)?;
        self.cursors.mark_declared(id);
        Ok(CursorHandle { declared: true, ..handle })
    }

    /// Declare a held cursor over `body` and return its first window — the path for
    /// a statement the wrapped `LIMIT` could not be applied to.
    async fn open_cursor(
        &self,
        body: &str,
        window: u32,
        seq: u64,
        started: Instant,
    ) -> DbResult<ExecuteResult> {
        let estimated_rows = self.estimated_rows(body).await;
        let types = self.column_types(body).await;
        self.check_cancelled(seq)?;

        let id = self.cursors.next_id();
        self.client.simple_query(&cursor::declare_cursor(&id, body)).await.map_err(map_pg)?;

        let handle = CursorHandle {
            name: id.clone(),
            types: types.clone(),
            body: body.to_string(),
            declared: true,
        };
        for evicted in self.cursors.register(&id, handle, Instant::now()) {
            if evicted.declared {
                self.close_cursor(&evicted.name).await;
            }
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
                    total_rows: None,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    end_of_result,
                    affected: None,
                    masked_columns: Vec::new(),
                })
            }
            Err(e) => {
                // A declared cursor whose first window failed is exactly the leak
                // this whole module exists to prevent. Deregistered *before* the
                // `CLOSE`, so a sweep running alongside cannot decide to close the
                // same one twice.
                if let Some(gone) = self.cursors.remove(&id) {
                    if gone.declared {
                        self.close_cursor(&gone.name).await;
                    }
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
                masked_columns: Vec::new(),
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
            masked_columns: Vec::new(),
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
        // Timed, per query, on stderr.
        //
        // Not for tuning: this is the one call in the product that can appear to
        // hang, because it is issued automatically the moment a connection opens
        // and the interface has nothing to show until it lands. When it is slow the
        // only question worth answering is *which* of the three is slow, and having
        // to add a print to find that out — on somebody else's database, which is
        // the only place it happens — costs a round of "try this build".
        let timed = |what: &'static str, began: Instant| {
            let took = began.elapsed();
            if took.as_millis() >= 500 {
                eprintln!("picus: reading {what} of `{schema}` took {}ms", took.as_millis());
            }
        };

        let at = Instant::now();
        let (tables, views) = bounded("relations", catalog::read_relations(&self.client, schema)).await?;
        timed("relations", at);

        let at = Instant::now();
        let sequences = bounded("sequences", catalog::read_sequences(&self.client, schema)).await?;
        timed("sequences", at);

        let at = Instant::now();
        let triggers = bounded("triggers", catalog::read_triggers(&self.client, schema)).await?;
        timed("triggers", at);

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

    async fn trigger_detail(&self, name: &str) -> DbResult<TriggerDetail> {
        catalog::read_trigger_detail(&self.client, self.schema(), name)
            .await?
            .ok_or_else(|| DbError::NotFound(format!("trigger {name}")))
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
            ExecutionPlan::Cursor(body) => self.open_result(body, window, seq, started).await,
            ExecutionPlan::Direct => self.run_direct(sql, window, seq, started).await,
        }
    }

    async fn open_relation(&self, relation: &str, window: u32) -> DbResult<ExecuteResult> {
        // Deliberately the *same* path as a typed query, not a parallel one: a
        // relation tab and a query tab must scroll identically, and the only thing
        // that differs is who wrote the SELECT. The quoting is the engine's, which
        // is the whole reason this is a method rather than a string the caller
        // composes — and, now that `execute` is where the masking lives, the only
        // thing this method still contributes.
        self.execute(&cursor::relation_query(self.schema(), relation), window).await
    }

    async fn result_window(
        &self,
        result_id: &str,
        offset: u64,
        limit: u32,
    ) -> DbResult<ResultWindow> {
        self.sweep_idle().await;
        let handle = self.cursors.touch(result_id, Instant::now()).ok_or_else(unknown_result)?;
        // The moment somebody asks for a row the first window did not hold is the
        // moment the cursor is worth what it costs — and not one statement earlier.
        let handle = match handle.declared {
            true => handle,
            false => self.declare_now(result_id, handle).await?,
        };
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

        // A result nobody has scrolled past is not a cursor yet, and counting it
        // must not make it one: declaring would materialise every row — including
        // every megabyte of every large object — to produce one number that does
        // not need any of them. `count(*)` over the same statement reads no columns
        // at all.
        //
        // It answers about a different execution than the rows on screen, which is
        // exactly what makes it the wrong thing to do over a held cursor and the
        // right thing here: there is no snapshot yet for it to disagree with, and
        // the number it replaces is the planner's guess.
        if !handle.declared {
            let counted = self.run_simple(&cursor::count_query(&handle.body), None, 1).await?;
            self.check_cancelled(seq)?;
            let total = counted
                .rows
                .first()
                .and_then(|row| row.first())
                .and_then(|cell| match cell {
                    CellValue::Int(n) => Some(*n),
                    CellValue::Text(t) => t.parse().ok(),
                    _ => None,
                })
                .unwrap_or(0);
            return Ok(ResultCount { total });
        }

        let fetched = self
            .run_simple(&cursor::count_statements(&handle.name), None, 0)
            .await?;
        self.check_cancelled(seq)?;

        // `MOVE FORWARD ALL` reports how far it moved — the exact row count of this
        // very result, not of a re-execution that might disagree with it.
        Ok(ResultCount { total: fetched.last_command_count.unwrap_or(0) as i64 })
    }

    async fn close_result(&self, result_id: &str) -> DbResult<()> {
        // Only a cursor that exists is closed: `CLOSE` on a name the server never
        // heard of is an error, and most results never became one.
        if let Some(gone) = self.cursors.remove(result_id) {
            if gone.declared {
                self.close_cursor(&gone.name).await;
            }
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

    /// Close the session, releasing what it holds — **and never taking longer than
    /// [`CLOSING_LIMIT`] to do it**.
    ///
    /// The bound is the whole point. A `CLOSE` is SQL, and SQL on a connection that
    /// is already stuck queues behind whatever is stuck on it. Without the bound,
    /// closing a wedged connection hangs, and so does everything built on closing
    /// one: disconnecting, reconnecting, and the reset that exists precisely to
    /// rescue a user from this state. The cure would need the disease to be over.
    ///
    /// What is lost by giving up is a tuplestore the server reclaims when the socket
    /// dies anyway. What is gained is that the user can always get out.
    async fn close(&self) -> DbResult<()> {
        // Flagged first so `status()` stops claiming health while the closes run —
        // and in its own scope, because a lock guard must never cross an `await`.
        {
            *self.closed.lock().unwrap_or_else(|p| p.into_inner()) = true;
        }
        // Deregistered before anything is sent, so giving up below cannot leave a
        // handle behind that a later sweep would try to close a second time.
        let held: Vec<String> =
            self.cursors.drain().into_iter().filter(|h| h.declared).map(|h| h.name).collect();
        if held.is_empty() {
            return Ok(());
        }

        let closing = async {
            for name in &held {
                self.close_cursor(name).await;
            }
        };
        if tokio::time::timeout(CLOSING_LIMIT, closing).await.is_err() {
            eprintln!(
                "picus: this connection did not answer a CLOSE within {}s, so {} held \
                 result(s) were abandoned — the server releases them when the socket goes",
                CLOSING_LIMIT.as_secs(),
                held.len(),
            );
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
