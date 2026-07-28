# picus-db-postgres

The PostgreSQL implementation of [`picus-db-api`](../api), over `tokio-postgres`:
pure Rust (no `libpq` to ship) and the only driver that gives a real server-side
cancellation key — which is what makes the query editor's **Cancel** stop a running
statement rather than abandon it.

## Five decisions worth knowing before reading the code

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

**The row limit is applied by the server where it can be.** `sql::capped_statement`
wraps a single read in `SELECT * FROM (…) LIMIT n`, so the rows the user will not
see never leave the database. Wrapping rather than appending is what keeps the
statement's own `LIMIT` / `ORDER BY` / `UNION` meaning what it said, and the
`prepare` doubles as the validity probe: a wrap that does not prepare is discarded
and the user's own text is run, so an error message never quotes Picus's rewrite.
One row beyond the limit is requested, which is what turns "there is more" from a
guess into a fact. Statements that cannot sit in a `FROM` keep the client-side cap
as a backstop.

**Cancellation is remembered, not just sent.** The server's cancel key only
interrupts what is running at the instant it arrives, and `execute` is more than one
round trip — a Cancel landing in a gap between them would hit nothing and be lost,
and the statement would then run in full. `PgSession` pairs a run ordinal with a
cancelled ordinal so the request survives the gap; scoping it to an ordinal is what
stops a cancel arriving after a query finished from killing the next one.

## Layout

| Module | What it does |
|---|---|
| `provider` | opens sessions; spawns the connection task (skip that and the client hangs silently) |
| `session` | the live connection: schema, paged rows, execute, cancel |
| `catalog` | the `pg_catalog` queries — faster than `information_schema`, and the only place trigger bitmasks and expression-index columns exist. Note the single-relation entry points (`read_relation`, `read_estimated_rows`): a page turn must not read a whole catalogue |
| `sql` | pure helpers: quoting, statement classification, statement scanning + the server-side cap, trigger-bit decoding. Fully unit-tested without a database |
| `tls` | rustls + the OS trust store, so an internal corporate CA works with no bundle shipped |
| `error` | driver error → `DbError`, keeping the SQL position so the editor can place a squiggle |

## Tests

`sql`, `session`'s value mapping and the descriptor are covered by plain unit tests
— no server needed. Anything requiring a live PostgreSQL is not tested here; that is
the integration suite's job.
