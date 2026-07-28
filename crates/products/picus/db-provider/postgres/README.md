# picus-db-postgres

The PostgreSQL implementation of [`picus-db-api`](../api), over `tokio-postgres`:
pure Rust (no `libpq` to ship) and the only driver that gives a real server-side
cancellation key — which is what makes the query editor's **Cancel** stop a running
statement rather than abandon it.

## Six decisions worth knowing before reading the code

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
| `catalog` | the `pg_catalog` queries — faster than `information_schema`, and the only place trigger bitmasks and expression-index columns exist. Note the single-relation entry point (`read_relation`): opening one tab must not read a whole catalogue |
| `sql` | pure helpers: quoting, statement classification, statement scanning, trigger-bit decoding. Fully unit-tested without a database |
| `tls` | rustls + the OS trust store, so an internal corporate CA works with no bundle shipped |
| `error` | driver error → `DbError`, keeping the SQL position so the editor can place a squiggle |

## Tests

`sql`, `cursor` (both halves), `rows`' value mapping and the descriptor are covered
by plain unit tests — no server needed. Every string a cursor operation sends is
built by a pure function and asserted verbatim, including the two that only matter
when they are wrong: a body opening with a `--` comment must not comment out its own
`DECLARE`, and an absurd offset is clamped rather than handed to the server.
Anything requiring a live PostgreSQL is not tested here; that is the integration
suite's job.
