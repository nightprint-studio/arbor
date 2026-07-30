//! Where the time goes on an ordered read — measured against a real server,
//! because that is the only place it can be measured.
//!
//! **Ignored by default and credential-free.** It never runs as part of
//! `cargo test`, and everything about the connection — including *which relation
//! and which column* — comes from the environment, so this file names no table
//! belonging to anybody:
//!
//! ```text
//! PICUS_TEST_HOST=… PICUS_TEST_DB=… PICUS_TEST_USER=… PICUS_TEST_PASSWORD=… \
//! PICUS_TEST_TABLE=… PICUS_TEST_ORDER=… \
//!   cargo test -p picus-db-postgres --test live_ordered_read -- --ignored --nocapture
//! ```
//!
//! ## It reports timings, never contents
//!
//! Row counts, column counts and milliseconds. No value is printed and no name is
//! printed that did not arrive in an environment variable.
//!
//! ## The four reads are one experiment
//!
//! Each is the *same* relation read a slightly different way, so the differences
//! between them say which part of the path costs what:
//!
//! | # | read | what it isolates |
//! |---|---|---|
//! | 1 | no ordering | the floor: scan + transfer of one window |
//! | 2 | `ORDER BY`, unbounded by the user | the reported problem |
//! | 3 | `ORDER BY … LIMIT n` written by hand | whether *our* bound is the one that is missing — this one takes the path where Picus appends nothing |
//! | 4 | `ORDER BY` on one narrow column | whether the cost is the sort or the payload it carries |
//!
//! If 3 is quick and 2 is not, the bound Picus appends is not reaching the planner.
//! If 2 and 3 are both slow but 4 is quick, the sort is carrying the large objects.
//! If all four are slow, the floor is the scan and no bound will help.
//!
//! ## What it found, and why the probes go on past four
//!
//! None of the above. On a table of 6 793 rows every ordered read hung, while the
//! unordered one answered in 10 ms — and then: a sort of four constants was fine, a
//! sort of four *strings* was fine (so not the collation), `count(*)` over the whole
//! table was fine (so not the scan), and `ORDER BY ctid` — a full scan plus a real
//! sort — was fine in 14 ms.
//!
//! What was left was **how** the order was produced. `EXPLAIN` of the plain
//! statement showed `Sort` over `Seq Scan`; `EXPLAIN` of the same statement with a
//! small `LIMIT` — which makes the planner prefer a fast-starting plan, exactly as
//! declaring a **cursor** does — showed an `Index Scan` instead. That index scan
//! never returned. Forcing the sequential scan off, so nothing but the index could
//! serve a bare `count(*)`, hung as well; forcing index scans off ran the ordered
//! read in 27 ms.
//!
//! So: **the index could not be walked.** Not a slow query and not a Picus fault.
//!
//! It was read as a *corrupt* index, and that was wrong: it came back with no
//! `REINDEX`, when the machine was restarted. What clears on a restart and not on
//! `pg_cancel_backend` is a **stale lock in shared memory** — a buffer left pinned
//! by a backend that died mid-scan. Those waits are uninterruptible, which is why
//! cancelling did nothing, and invisible in `pg_locks`, which is why nothing named
//! the holder. This harness very likely made it worse, and may have caused it:
//! every read it abandoned left a backend scanning forever, until it learned to
//! cancel what it gives up on (see [`timed`]).
//!
//! **The half that survives, and is still reproducible on a healthy database**, is
//! the plan difference — the two `EXPLAIN`s above. Picus reads through a cursor,
//! PostgreSQL plans a cursor with `cursor_tuple_fraction` and therefore prefers a
//! plan that starts fast, so it takes the index. A client that limits rows on its
//! own side leaves the planner unbounded and gets the sort. That is why the same
//! statement met the wedged index from here and not from there — and why the other
//! client looked "a bit slower": it was sorting.
//!
//! The lesson worth keeping is the shape of the probe list: each read differs from
//! its neighbour in **one** thing, so the first pair that disagrees names the cause.

use std::time::{Duration, Instant};

use picus_db_api::prelude::*;
use picus_db_postgres::prelude::PostgresProvider;

/// How long any one read may take before the harness stops waiting on it. Long
/// enough that "slow" is measured rather than guessed, short enough to end.
const PATIENCE: Duration = Duration::from_secs(25);

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn spec() -> Option<(ConnectionSpec, Option<Secret>)> {
    let spec = ConnectionSpec {
        id: "live-ordered".to_string(),
        name: "live-ordered".to_string(),
        alias: String::new(),
        engine: EngineKind::Postgres,
        host: env("PICUS_TEST_HOST")?,
        port: env("PICUS_TEST_PORT").and_then(|p| p.parse().ok()).unwrap_or(5432),
        database: env("PICUS_TEST_DB")?,
        user: env("PICUS_TEST_USER")?,
        schema: env("PICUS_TEST_SCHEMA").unwrap_or_else(|| "public".to_string()),
        color_idx: 0,
        read_only: false,
        tls: false,
        script_root: None,
        params: Default::default(),
    };
    Some((spec, env("PICUS_TEST_PASSWORD").map(Secret::new)))
}

/// Run one statement through the **product's own path**, on a connection of its
/// own, and report its shape.
///
/// Deliberately `DbSession::execute` and not a hand-rolled query: the question is
/// what Picus does, so anything measured through a side door would be measuring
/// something else.
///
/// **A connection per read**, which is not fussiness. Giving up on a future does not
/// stop the statement on the server, and a session is one connection — so the read
/// after a slow one queues behind it and reports the same time for a reason that
/// has nothing to do with itself. The first version of this harness did that, and
/// three of its four numbers were meaningless.
async fn timed(spec: &ConnectionSpec, label: &str, sql: &str, window: u32) {
    let Ok(session) =
        PostgresProvider::default().connect(spec, env("PICUS_TEST_PASSWORD").map(Secret::new)).await
    else {
        println!("  {label:<34}      —      could not connect");
        return;
    };
    let session = &*session;
    let began = Instant::now();
    match tokio::time::timeout(PATIENCE, session.execute(sql, window)).await {
        Ok(Ok(result)) => {
            let took = began.elapsed().as_millis();
            if let Some(held) = &result.result_id {
                let _ = session.close_result(held).await;
            }
            println!(
                "  {label:<34} {took:>7} ms   rows {:<5} cols {:<4} masked {:<2} est {}",
                result.row_count,
                result.columns.as_ref().map(Vec::len).unwrap_or(0),
                result.masked_columns.len(),
                result.estimated_rows.map(|n| n.to_string()).unwrap_or_else(|| "?".into()),
            );
        }
        Ok(Err(e)) => println!("  {label:<34} {:>7}      FAILED: {e}", began.elapsed().as_millis()),
        Err(_) => {
            // Giving up on the future does **not** stop the statement: the server
            // keeps running it, holding its locks, until something says otherwise.
            // An earlier version of this file left thirty-nine such backends on a
            // database, which then blocked the very `REINDEX` that would have fixed
            // what it had just diagnosed. A harness that measures a hang has to be
            // able to end one.
            let _ = session.cancel().await;
            println!("  {label:<34} {:>7} ms   *** gave up, cancelled ***", PATIENCE.as_millis());
        }
    }
}

/// Say which databases the server does have, when the configured one is not one of
/// them. Best-effort throughout — this runs on a path that has already failed.
async fn list_databases(spec: &ConnectionSpec) {
    let mut maintenance = spec.clone();
    maintenance.database = "postgres".to_string();
    maintenance.schema = "public".to_string();
    let Ok(session) =
        PostgresProvider::default().connect(&maintenance, env("PICUS_TEST_PASSWORD").map(Secret::new)).await
    else {
        return;
    };
    // No ordering: the point is the list, and this is a diagnostic path that must
    // not itself become the thing that hangs.
    if let Ok(Ok(result)) = tokio::time::timeout(
        PATIENCE,
        session.execute("SELECT datname FROM pg_database WHERE NOT datistemplate", 200),
    )
    .await
    {
        let names: Vec<String> = result
            .rows
            .iter()
            .filter_map(|row| match row.first() {
                Some(CellValue::Text(t)) => Some(t.clone()),
                _ => None,
            })
            .collect();
        println!("  databases on this server: {}", names.join(", "));
        if let Some(held) = &result.result_id {
            let _ = session.close_result(held).await;
        }
    }
    let _ = session.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server; see the module docs"]
async fn where_the_time_goes_on_an_ordered_read() {
    let Some((spec, secret)) = spec() else {
        println!("PICUS_TEST_* not set — skipping.");
        return;
    };
    let (Some(table), Some(order)) = (env("PICUS_TEST_TABLE"), env("PICUS_TEST_ORDER")) else {
        println!("PICUS_TEST_TABLE / PICUS_TEST_ORDER not set — skipping.");
        return;
    };

    println!("connecting to {}:{}/{} …", spec.host, spec.port, spec.database);
    let began = Instant::now();
    match PostgresProvider::default().connect(&spec, secret).await {
        Ok(session) => {
            println!("  connected in {} ms\n", began.elapsed().as_millis());
            let _ = session.close().await;
        }
        Err(e) => {
            println!("connect failed: {e}");
            // Which is nearly always the database name or the wrong listener, and
            // "it does not exist" without saying what does exist costs a round trip
            // through the person running this.
            list_databases(&spec).await;
            return;
        }
    }

    let window = 500u32;
    timed(&spec, "1 · no ordering", &format!("SELECT * FROM {table}"), window).await;
    timed(&spec, "2 · ORDER BY, unbounded", &format!("SELECT * FROM {table} ORDER BY {order}"), window)
        .await;
    timed(
        &spec,
        "3 · ORDER BY, bounded by hand",
        &format!("SELECT * FROM {table} ORDER BY {order} LIMIT {}", window + 1),
        window,
    )
    .await;
    timed(
        &spec,
        "4 · ORDER BY, one column only",
        &format!("SELECT {order} FROM {table} ORDER BY {order}"),
        window,
    )
    .await;
    // The floor for "does this server sort at all": four rows of a constant, with
    // no table and no storage involved. If even this does not come back, nothing
    // about the statement is the problem.
    timed(&spec, "5 · sort of four constants", "SELECT * FROM (VALUES (3),(1),(2),(4)) v(n) ORDER BY n", window)
        .await;
    // …and of four *text* constants, which is the same sort plus a collation. The
    // pair tells a broken collation apart from anything else.
    timed(
        &spec,
        "6 · sort of four strings",
        "SELECT * FROM (VALUES ('b'),('a'),('d'),('c')) v(t) ORDER BY t",
        window,
    )
    .await;
    // THE discriminator between the scan and the sort. Read 1 stops after one
    // window, so it never touches most of the table; an ordering has to read every
    // row before it can return one. This reads every row and sorts nothing.
    timed(&spec, "7 · full scan, no ordering", &format!("SELECT count(*) FROM {table}"), window).await;
    // A full scan plus the cheapest possible sort: `ctid` is a physical address,
    // with no collation and no comparison worth the name. It also cannot be served
    // by an index, which turns out to be the whole point.
    timed(&spec, "8 · ordered by ctid", &format!("SELECT * FROM {table} ORDER BY ctid"), window).await;

    // If the scan, the sort and the collation are all sound, what is left is *how*
    // the planner chose to produce the order — and the cheap way is to walk an
    // index instead of sorting. These two say whether that is it.
    explain(&spec, &format!("SELECT * FROM {table} ORDER BY {order}")).await;
    // The same question asked the way a CURSOR asks it. A cursor is planned with
    // `cursor_tuple_fraction` — the planner is told only a fraction of the rows
    // will be fetched, so it prefers a plan that starts returning them at once
    // over one that is cheapest overall. A small `LIMIT` produces the same
    // preference, which is how the cursor's plan can be seen without declaring one.
    explain(&spec, &format!("SELECT * FROM {table} ORDER BY {order} LIMIT 10")).await;
    // The index on its own, with no ordering and no cursor anywhere near it:
    // forcing the sequential scan off leaves the planner nothing but the index to
    // count through. If *this* does not come back, the index is unwalkable and
    // everything above is a consequence rather than a cause.
    timed(
        &spec,
        "9 · count through the index",
        &format!("SET LOCAL enable_seqscan = off; SELECT count(*) FROM {table}"),
        window,
    )
    .await;
    timed(
        &spec,
        "10 · ordered, index scans off",
        &format!(
            "SET LOCAL enable_indexscan = off; SET LOCAL enable_indexonlyscan = off; \
             SELECT * FROM {table} ORDER BY {order}"
        ),
        window,
    )
    .await;
}

/// Print the plan the server would use. Planning only — `EXPLAIN` never executes
/// the statement, so this answers even when running it would not.
async fn explain(spec: &ConnectionSpec, sql: &str) {
    let Ok(session) =
        PostgresProvider::default().connect(spec, env("PICUS_TEST_PASSWORD").map(Secret::new)).await
    else {
        return;
    };
    match tokio::time::timeout(PATIENCE, session.execute(&format!("EXPLAIN {sql}"), 40)).await {
        Ok(Ok(result)) => {
            println!("\n  the plan for the ordered read:");
            for row in &result.rows {
                if let Some(CellValue::Text(line)) = row.first() {
                    println!("    {line}");
                }
            }
            println!();
            if let Some(held) = &result.result_id {
                let _ = session.close_result(held).await;
            }
        }
        Ok(Err(e)) => println!("  EXPLAIN failed: {e}"),
        Err(_) => println!("  EXPLAIN did not return — even planning is stuck"),
    }
    let _ = session.close().await;
}
