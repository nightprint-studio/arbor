//! A live connection, timed — the harness that found "reading schema never comes
//! back", kept because the next one of those will want it too.
//!
//! **Ignored by default and credential-free.** It talks to a real server, so it is
//! never part of `cargo test`, and it takes its connection from the environment so
//! no password is ever written into this repository:
//!
//! ```text
//! PICUS_TEST_HOST=localhost PICUS_TEST_DB=… PICUS_TEST_USER=… PICUS_TEST_PASSWORD=… \
//!   cargo test -p picus-db-postgres --test live_schema -- --ignored --nocapture
//! ```
//!
//! ## It reports shapes, never contents
//!
//! Counts, timings and the outcome. No table name, no column name and no row is
//! printed: the question is where the time goes, and a dump of somebody's catalogue
//! into a terminal is not part of answering it.
//!
//! ## The two tests are one experiment
//!
//! `directly` runs the read the ordinary way — inside a Tokio runtime, awaiting.
//! `through_block_on` runs it the way the **backend** does: a plain OS thread
//! calling `Handle::block_on` on a shared multi-thread runtime, which is what
//! `arbor-be`'s dispatcher does for every async handler. If one is fast and the
//! other hangs, the fault is in how the backend dispatches rather than in the query
//! — a distinction that cannot be made from inside the application.
//!
//! ## What it caught
//!
//! A catalogue read that never returned. Everything pointed at the query being slow;
//! it was not. Unsorted it streamed 15 222 rows in 45 ms, and with **any** `ORDER BY`
//! — including by `oid`, which has no collation to blame — it did not come back at
//! all. The ordering now happens in Rust. That schema went from never to 400 ms.

use std::time::{Duration, Instant};

use picus_db_api::prelude::*;
use picus_db_postgres::prelude::PostgresProvider;

/// The connection under test, from the environment. `None` when it is not set,
/// which is how the tests skip instead of failing on a machine with no server.
fn spec() -> Option<(ConnectionSpec, Option<Secret>)> {
    let host = std::env::var("PICUS_TEST_HOST").ok()?;
    let database = std::env::var("PICUS_TEST_DB").ok()?;
    let user = std::env::var("PICUS_TEST_USER").ok()?;
    let password = std::env::var("PICUS_TEST_PASSWORD").ok();

    let spec = ConnectionSpec {
        id: "live-test".to_string(),
        name: "live-test".to_string(),
        alias: String::new(),
        engine: EngineKind::Postgres,
        host,
        port: std::env::var("PICUS_TEST_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5432),
        database,
        user,
        schema: std::env::var("PICUS_TEST_SCHEMA").unwrap_or_else(|_| "public".to_string()),
        color_idx: 0,
        read_only: false,
        tls: false,
        script_root: None,
        params: Default::default(),
    };
    Some((spec, password.map(Secret::new)))
}

/// What a phase cost, printed without saying anything about what it read.
fn phase(what: &str, began: Instant) {
    println!("  {what:<28} {:>8} ms", began.elapsed().as_millis());
}

/// Connect, read the schema, and report the shape of what came back.
async fn measure() {
    let Some((spec, secret)) = spec() else {
        println!("PICUS_TEST_* not set — skipping.");
        return;
    };
    println!("connecting to {}:{}/{} …", spec.host, spec.port, spec.database);

    let began = Instant::now();
    let session = PostgresProvider::default().connect(&spec, secret).await.expect("connect");
    phase("connect", began);

    let began = Instant::now();
    let schema = match tokio::time::timeout(Duration::from_secs(120), session.read_schema()).await {
        Ok(Ok(schema)) => schema,
        Ok(Err(e)) => panic!("read_schema failed: {e}"),
        Err(_) => panic!("read_schema did not return within 120s — it is not slow, it is stuck"),
    };
    phase("read_schema", began);

    // Shapes only. The column count is the one number that matters for the
    // catalogue query, because it *is* that query's row count.
    let columns: usize =
        schema.tables.iter().chain(schema.views.iter()).map(|t| t.columns.len()).sum();
    println!(
        "  tables {} · views {} · sequences {} · triggers {} · {columns} columns in total",
        schema.tables.len(),
        schema.views.len(),
        schema.sequences.len(),
        schema.triggers.len(),
    );

    // Asserted rather than eyeballed: the ordering used to be the server's, and
    // taking it back into Rust is what made this read return at all.
    assert!(
        schema.tables.windows(2).all(|w| w[0].name <= w[1].name),
        "tables must come back sorted by name",
    );

    let _ = session.close().await;
}

/// The ordinary path: inside a runtime, awaited.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server; see the module docs"]
async fn directly() {
    measure().await;
}

/// The backend's path: an OS thread blocking on a shared multi-thread runtime.
///
/// This is what `arbor-be`'s dispatcher does for every async handler — the serve
/// loop hands each request to a plain `std::thread`, which calls `Handle::block_on`.
/// The connection driver `tokio::spawn`s inside that, onto the same runtime, and
/// every later query needs it to keep being polled.
#[test]
#[ignore = "needs a live server; see the module docs"]
fn through_block_on() {
    let runtime =
        tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    let handle = runtime.handle().clone();

    // A thread that is NOT a runtime worker, exactly as the serve loop's is.
    std::thread::spawn(move || handle.block_on(measure()))
        .join()
        .expect("the dispatch thread panicked");
}
