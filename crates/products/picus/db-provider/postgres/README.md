# picus-db-postgres

The PostgreSQL implementation of [`picus-db-api`](../api), over `tokio-postgres`:
pure Rust (no `libpq` to ship) and the only driver that gives a real server-side
cancellation key — which is what makes the query editor's **Cancel** stop a running
statement rather than abandon it.

## Nine decisions worth knowing before reading the code

**Values come back as the server's own text.** Execution uses the *simple query
protocol*, so a `timestamptz`, a wide `numeric` and a domain type arrive looking
exactly as the server prints them — which is what the user is about to paste into a
script. Only columns the server reports as numeric become numbers, so the grid can
right-align them; a `varchar` holding `007` stays `007`, and a decimal too precise
for an `f64` stays text rather than being silently rounded. Type information comes
from a best-effort `prepare` (some perfectly good statements aren't preparable, and
then the columns are simply untyped). `NULL` never collapses into the empty string.

**Read-only is enforced by the server.** A read-only session is opened with
`SESSION CHARACTERISTICS AS TRANSACTION READ ONLY`, so the refusal holds for a
pasted script or a plugin, not only for the buttons the UI greys out. The lexical
check in `sql::guard_read_only` exists to produce a better message sooner, and errs
toward calling an unknown verb a write.

**Object names are quoted, never interpolated raw.** A relation name cannot be a
bind parameter, so `sql::quote_ident` is the one thing between a hostile table name
and an executed statement. It has the test to prove it.

**A read is a held cursor.** `execute` declares a `SCROLL CURSOR WITH HOLD` over the
statement and returns its first window; the grid scrolls by asking for windows
against the `resultId`. Not `OFFSET` paging: without an explicit `ORDER BY`,
PostgreSQL is free to return the same rows in a different sequence the second time,
so paging by offset can show a row twice in one window and never in the next while
the user does nothing but scroll. One row beyond the window is fetched every time,
which is what turns "there is more" from a guess into a fact. Statements a cursor
cannot be declared over (`SHOW`, `EXPLAIN`, a write, a multi-statement paste) run
exactly as typed, hold nothing, and say so by coming back with `resultId: null`.

**The first window is not the held cursor.** Declaring `WITH HOLD` up front meant every
read waited for the *whole* result to be copied server-side before a single row
appeared — and the grid's row limit bounded none of it, so a table of scanned documents
took minutes to show five hundred rows with nothing on screen and a Cancel that had a
commit to interrupt rather than a query. The first window is now read through a cursor
**without** `HOLD`, declared and closed inside one `BEGIN … COMMIT` that crosses the
wire as a single string — that form streams, so it costs the rows it returns. The
holdable cursor is declared only when somebody asks for a row the first window did not
hold; an exact count asked of a result that never got one goes through `count(*)` over
the same statement, which reads none of the columns. The bound goes into the **statement**, not only into the
`FETCH`: a `FETCH FORWARD 501` is invisible to the planner, so `SELECT … ORDER BY x`
sorted the entire table to a temporary file before returning a row — which is why an
ordered query answered in seconds elsewhere and appeared to hang here. With a real
`LIMIT` the same sort becomes a top-N. It is appended only when it can be
(`bounded_body` refuses a statement that already carries `LIMIT`, `OFFSET`,
`FETCH FIRST` or a locking clause). The cost that remains: the first window and the
holdable cursor are two executions, so without an `ORDER BY` a row may repeat or be
skipped exactly at that boundary. The old guarantee only held for people who waited
out the materialisation, and nobody did.

**A cancelled first window is not a failed one.** The streamed read falls back to the
holdable cursor when it fails — a statement the user got wrong, a masking wrapper that
would not apply — and that fallback re-runs the statement. A cancellation arrives on
the same path and must not take it: pressing Cancel would start, the slow way, the
very query being escaped from, on a tab that had already reported itself cancelled.
The cancelled ordinal is therefore checked inside the error arm, after the `ROLLBACK`
that keeps the session usable and before the second attempt.

**Ordering wins over masking.** Masking means wrapping the statement in a subquery,
and PostgreSQL does not have to hand a sub-select's rows on in the order it produced
them — a parallel plan uses `Gather` rather than `Gather Merge` and interleaves them.
So a statement with a top-level `ORDER BY` is never wrapped: it carries its large
objects, bounded to the window being read. A grid in the wrong order is a wrong
answer; a slow read is a slow read.

**`WITH HOLD` is a trade, and it is made deliberately.** The alternative — a cursor
inside a transaction held open for the life of the tab — is unavailable here: a
session is *one* backend shared by every query tab, so an open transaction would
enclose every other tab's statement and one syntax error would abort it and take
every result down with it; there is no second connection to put it on, because Picus
holds no password and cannot reconnect; and an open snapshot blocks vacuuming for
hours. The price of `WITH HOLD` is that the server runs the query to completion and
copies the whole result into a tuplestore before the first window returns — memory
up to `work_mem`, a temp file beyond it. Paid once, it buys stable windows in both
directions, an exact count that walks storage instead of re-scanning the table, and
no silent truncation. Nothing is capped: a cap is the truncation this replaced.

**Large objects are never fetched to draw a grid, however the statement asked for
them.** A `bytea` column of scanned documents costs minutes and gigabytes to draw a
grid that can show none of it, so a result carrying one is read through a projection
where they stand for themselves. **Which** columns those are comes from the server's
own description of the result — the `prepare` that already types the columns — not
from reading the SQL, so it holds for a join, a union, a CTE, and for
`SELECT allegato FROM archivio` exactly as for `SELECT *`. An earlier version
recognised only `SELECT * FROM <one table>`, which meant naming the column was a way
round it. The rewrite is a wrapper (`masked_source`), so nothing about the
statement's own shape has to be understood; it is skipped when a column name is
duplicated or empty, because then the wrapper could not name one. The size shown is
`pg_column_size`, which reads the TOAST pointer rather than detoasting the value; for
compressible data it under-reports, and the exact length comes back with the value
when the cell is opened. It is deliberately *not* conditional on the row being
addressable: a grid of sizes you cannot open is a smaller problem than a read nobody
can wait out, and selecting the key column fixes it.

**A bound value is never spliced into the statement, and a bound read never
scrolls.** `execute_bound` sends the values beside the SQL, in text format, so the
server parses each one with the input function of the type *it* inferred for that
placeholder — no quoting on this side, and a bad value comes back in the server's own
words. The cost is stated rather than hidden: `DECLARE … CURSOR` is a utility
statement and takes no parameters, so there is nothing to hold the result over. A
parameterised read returns up to `window` rows with `result_id: None` and an honest
`end_of_result`, and the interface says so. Parameters also force the extended
protocol, where the driver asks for binary results — so the projection is cast to
`text` through a wrapper (a bounded subquery for a query, a CTE for a
`RETURNING` write) rather than PostgreSQL's output functions being re-implemented
here. The bound goes **inside** that wrapper: a subquery carrying no `LIMIT` is
pulled up and loses its `ORDER BY`.

**Cancellation is remembered, not just sent.** The server's cancel key only
interrupts what is running at the instant it arrives, and `execute` is more than one
round trip — a Cancel landing in a gap between them would hit nothing and be lost,
and the statement would then run in full. `PgSession` pairs a run ordinal with a
cancelled ordinal so the request survives the gap; scoping it to an ordinal is what
stops a cancel arriving after a query finished from killing the next one. The exact
row count takes an ordinal like anything else, which is what makes it cancellable.

**A cursor nobody closes is a leak on somebody's production database**, so four
things close one: an explicit `close_result`; eviction, when a session already holds
`MAX_OPEN` (16) and opens another — least recently used first; expiry, after
`IDLE_TTL` (30 minutes) with nothing asking it anything; and the session closing.
Expiry is swept at the start of every statement on that connection rather than by a
timer: a background thread issuing SQL on a connection somebody else is using is a
worse hazard than a tuplestore that lives until disconnect. The consequence, stated
rather than hidden: a connection nobody touches again keeps its results until it is
closed or the backend exits.

## Layout

| Module | What it does |
|---|---|
| `provider` | opens sessions; spawns the connection task (skip that and the client hangs silently) |
| `session` | the live connection: schema, execute, windows, counting, cancel |
| `cursor` | held results — the `DECLARE` / `MOVE` / `FETCH` / `CLOSE` construction (`cursor::sql`, pure) and the per-session registry that decides when one dies (`cursor::registry`). Read its module docs before changing anything about `WITH HOLD` |
| `rows` | the reply → columns + cells mapping: server text in, `CellValue` out |
| `origins` | which relation each result column is read from. Taken from the `RowDescription` of the `prepare` that already happens, so it costs no round trip and — unlike any reading of the SQL — is right about a `*`, an alias and a view. Oids are resolved to names once per session and cached, negatives included. Every failure is silence: it is decoration, and decoration must not fail a query that produced rows |
| `bind` | statements whose values travel beside them: the text-format parameter encoding, and the wrapper that reads the reply back as the server's own text. Its module docs state the limit — a bound read holds no cursor — and why the `LIMIT` has to sit inside the wrapper |
| `catalog` | the `pg_catalog` queries — faster than `information_schema`, and the only place trigger bitmasks and expression-index columns exist. Note the single-relation entry point (`read_relation`): opening one tab must not read a whole catalogue. A sequence bound equal to the type's own extreme is sent as `None`: it is not this sequence's limit, and `i64::MAX` does not survive a JSON number anyway. `read_trigger_detail` is lazy for the same reason as `read_relation`, and reaches `pg_get_functiondef` only through a `CASE` that has already checked the language — it *raises* on a C or `internal` routine |
| `activity` | the session monitor: `pg_stat_activity`, `pg_blocking_pids()` and the lock being waited for in **one** statement — read separately, the blocking graph describes a moment the session list has already left. Every age is computed by the server in milliseconds (`clock_timestamp()`, never `now()`): subtracting the server's timestamps from the browser's clock is wrong by however far the two machines disagree, and wrong invisibly. `stop_session` carries the server's own boolean and the server's own refusal — a silent no-op reads as "Terminate does nothing" |
| `tx` | explicit transactions — and **no flag of its own**: every call reads the state from the server, because a statement that fails inside a block aborts the transaction without anyone asking, and a client-side boolean would still be saying "active" while the connection accepted nothing. `tokio-postgres` does not expose the protocol's transaction-status byte, so the state is deduced from `transaction_timestamp() <> statement_timestamp()` plus the `25P02` an aborted block answers with. `commit` **refuses** a failed transaction rather than forwarding it: PostgreSQL's own `COMMIT` there performs a rollback and reports success |
| `sql` | pure helpers: quoting, statement classification, statement scanning, trigger-bit decoding. Fully unit-tested without a database |
| `tls` | rustls + the OS trust store, so an internal corporate CA works with no bundle shipped |
| `error` | driver error → `DbError`, keeping the SQL position so the editor can place a squiggle |

## Tests

`sql`, `cursor` (both halves), `rows`' value mapping and the descriptor are covered
by plain unit tests — no server needed. Every string a cursor operation sends is
built by a pure function and asserted verbatim, including the two that only matter
when they are wrong: a body opening with a `--` comment must not comment out its own
`DECLARE`, and an absurd offset is clamped rather than handed to the server.
The star-select rewrite is tested from both sides, and at greater length from the
side that matters: what it must *leave alone*.

Two harnesses under `tests/` do need a live server, and neither runs by default —
they are `#[ignore]`d, and they take their connection from `PICUS_TEST_*`
environment variables so no credential is ever written into this repository.
`live_schema` times a catalogue read (it is what caught a schema read that never
returned); `live_large_objects` creates a throwaway schema of its own and proves that
a `bytea` column is not fetched to draw a grid however the statement asked for it,
then drops the schema again; `live_ordered_read` bisects a slow ordered read by
running the same relation eight ways that differ in one thing each — it is what
established that an ordered read hanging here and not in another client was an
**unwalkable index**, met from here and not from there because a cursor is planned
to start fast and therefore prefers the index where a plain query sorts instead.
Note what it does on giving up: it *cancels*. Abandoning the future leaves the
server running the statement, and an earlier version of that file left thirty-nine
such backends on a database.
