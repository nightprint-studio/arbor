//! The promise that a large object is **not** fetched to draw a grid — checked
//! against a real server, because that is the only place it can be checked.
//!
//! **Ignored by default and credential-free.** It talks to a live PostgreSQL, so it
//! is never part of `cargo test`, and it takes its connection from the environment
//! so no password is ever written into this repository:
//!
//! ```text
//! PICUS_TEST_HOST=localhost PICUS_TEST_DB=… PICUS_TEST_USER=… PICUS_TEST_PASSWORD=… \
//!   cargo test -p picus-db-postgres --test live_large_objects -- --ignored --nocapture
//! ```
//!
//! ## It brings its own schema and takes it away again
//!
//! Nothing here reads, writes or names anything that was already on the server. The
//! test creates a schema of its own, fills it with two invented tables, and drops it
//! at the end — including when an assertion fails, because a test that leaves
//! wreckage behind on somebody's database is a worse test than one that does not
//! run. The results are gathered first and asserted *after* the cleanup for exactly
//! that reason.
//!
//! ## What it is actually asserting
//!
//! Four properties, and the third and fourth are the ones worth having:
//!
//! 1. a `bytea` column comes back as a **size**, and the column is named in
//!    `masked_columns` so the interface knows to offer opening it;
//! 2. a typed `SELECT * FROM …` gets the same treatment as the relation tab — one
//!    behaviour, not two;
//! 3. **naming the column changes nothing.** `SELECT allegato FROM …` is masked
//!    exactly as `SELECT *` is. This is the property that had to be learned twice:
//!    the masking is driven by what the *result* contains, not by how the statement
//!    was written, so no way of asking drags the bytes across;
//! 4. a table with **no primary key** is masked too. Its cells cannot be opened —
//!    there is nothing to address a row by — but a grid of sizes you cannot open is
//!    a smaller problem than a read that takes minutes and cannot be cancelled.

use std::time::Instant;

use picus_db_api::prelude::*;
use picus_db_postgres::prelude::PostgresProvider;

/// The throwaway schema. Distinctive on purpose: the cleanup drops a schema by
/// name, and it must be impossible for that name to be one somebody wanted.
const PROBE_SCHEMA: &str = "picus_probe_large_objects";

/// Rows of a quarter-megabyte each — enough for the unmasked read to be visibly the
/// slower one, small enough that the test stays a test.
const ROWS: usize = 40;
const PAYLOAD_BYTES: usize = 256 * 1024;

/// A connection from the environment, or `None` when it is not configured — which
/// is how this skips instead of failing on a machine with no server.
fn spec(schema: &str) -> Option<(ConnectionSpec, Option<Secret>)> {
    let spec = ConnectionSpec {
        id: "live-lob-test".to_string(),
        name: "live-lob-test".to_string(),
        alias: String::new(),
        engine: EngineKind::Postgres,
        host: std::env::var("PICUS_TEST_HOST").ok()?,
        port: std::env::var("PICUS_TEST_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5432),
        database: std::env::var("PICUS_TEST_DB").ok()?,
        user: std::env::var("PICUS_TEST_USER").ok()?,
        schema: schema.to_string(),
        color_idx: 0,
        read_only: false,
        tls: false,
        script_root: None,
        params: Default::default(),
    };
    Some((spec, std::env::var("PICUS_TEST_PASSWORD").ok().map(Secret::new)))
}

/// The tables the probe works on. Invented, and deliberately unlike anything in a
/// real repository: `bacheca` has a key and can therefore be masked, `bacheca_aperta`
/// has none and therefore must not be.
fn setup_statements() -> Vec<String> {
    vec![
        format!("DROP SCHEMA IF EXISTS {PROBE_SCHEMA} CASCADE"),
        format!("CREATE SCHEMA {PROBE_SCHEMA}"),
        format!(
            "CREATE TABLE {PROBE_SCHEMA}.bacheca (
                 matricola integer PRIMARY KEY,
                 etichetta text NOT NULL,
                 allegato  bytea)"
        ),
        // EXTERNAL, so the payload is stored uncompressed and `pg_column_size`
        // agrees with the real length. With the default (EXTENDED) a run of
        // identical bytes compresses to almost nothing, which would make the size
        // assertion below prove something other than what it says.
        format!("ALTER TABLE {PROBE_SCHEMA}.bacheca ALTER COLUMN allegato SET STORAGE EXTERNAL"),
        format!(
            "INSERT INTO {PROBE_SCHEMA}.bacheca (matricola, etichetta, allegato)
             SELECT g, 'riga ' || g, decode(repeat('a1b2', {}), 'hex')
               FROM generate_series(1, {ROWS}) AS g",
            PAYLOAD_BYTES / 2
        ),
        format!(
            "CREATE TABLE {PROBE_SCHEMA}.bacheca_aperta (
                 etichetta text,
                 allegato  bytea)"
        ),
        format!(
            "INSERT INTO {PROBE_SCHEMA}.bacheca_aperta (etichetta, allegato)
             SELECT 'riga ' || g, decode(repeat('a1b2', 64), 'hex')
               FROM generate_series(1, 5) AS g"
        ),
    ]
}

/// What one read produced, reduced to the three things being asserted.
#[derive(Debug)]
struct Read {
    masked: Vec<String>,
    /// The first row's `allegato`, as text — a number when it was masked, a hex
    /// dump when it was not.
    first: String,
    millis: u128,
}

async fn read_of(
    session: &dyn DbSession,
    what: &str,
    at: usize,
    run: impl std::future::Future<Output = DbResult<ExecuteResult>>,
) -> DbResult<Read> {
    let began = Instant::now();
    let result = run.await?;
    let millis = began.elapsed().as_millis();
    if let Some(held) = &result.result_id {
        let _ = session.close_result(held).await;
    }
    let first = match result.rows.first().and_then(|row| row.get(at)) {
        Some(CellValue::Text(t)) => t.clone(),
        Some(CellValue::Int(n)) => n.to_string(),
        Some(CellValue::Float(n)) => n.to_string(),
        other => format!("{other:?}"),
    };
    println!("  {what:<34} {millis:>6} ms   masked={:?}", result.masked_columns);
    Ok(Read { masked: result.masked_columns, first, millis })
}

/// Everything the run learned, so the schema can be dropped before anything is
/// asserted.
#[derive(Debug)]
struct Findings {
    tab: Read,
    typed: Read,
    by_name: Read,
    keyless: Read,
}

async fn probe() -> DbResult<Findings> {
    // Session one: the DDL, against whatever schema the environment names.
    let (admin_spec, secret) = spec("public").expect("checked by the caller");
    println!("connecting to {}:{}/{} …", admin_spec.host, admin_spec.port, admin_spec.database);
    let admin = PostgresProvider::default().connect(&admin_spec, secret).await?;
    for statement in setup_statements() {
        let began = Instant::now();
        // The first six words: enough to say which statement, and it never reaches
        // into the data. Printed per statement because the setup is where a wedged
        // server shows itself, and "it hung" is not a useful thing to find out.
        let head: String = statement.split_whitespace().take(6).collect::<Vec<_>>().join(" ");
        admin.execute(&statement, 1).await?;
        println!("  {head:<52} {:>6} ms", began.elapsed().as_millis());
    }
    let _ = admin.close().await;

    // Session two: the probe's own schema, which is what the masking reads against.
    let (probe_spec, secret) = spec(PROBE_SCHEMA).expect("checked by the caller");
    let session = PostgresProvider::default().connect(&probe_spec, secret).await?;

    let findings = async {
        Ok::<_, DbError>(Findings {
            // The relation tab.
            tab: read_of(&*session, "tab on the keyed table", 2, session.open_relation("bacheca", 25))
                .await?,
            // The same read, typed by hand. It must behave identically.
            typed: read_of(
                &*session,
                "typed SELECT *",
                2,
                session.execute("SELECT * FROM bacheca ORDER BY matricola", 25),
            )
            .await?,
            // Asked for by name: this one must arrive in full.
            by_name: read_of(
                &*session,
                "the column asked for by name",
                0,
                session.execute("SELECT allegato FROM bacheca ORDER BY matricola", 25),
            )
            .await?,
            // No key, so nothing to read a value back by.
            keyless: read_of(
                &*session,
                "tab on the keyless table",
                1,
                session.open_relation("bacheca_aperta", 25),
            )
            .await?,
        })
    }
    .await;

    let _ = session.close().await;
    findings
}

/// Drop the probe schema. Best-effort and unconditional — the point is that it runs
/// even when the run above failed.
async fn cleanup() {
    let Some((admin_spec, secret)) = spec("public") else { return };
    let Ok(admin) = PostgresProvider::default().connect(&admin_spec, secret).await else { return };
    let _ = admin.execute(&format!("DROP SCHEMA IF EXISTS {PROBE_SCHEMA} CASCADE"), 1).await;
    let _ = admin.close().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live server; see the module docs"]
async fn a_large_object_is_a_size_until_it_is_asked_for() {
    if spec("public").is_none() {
        println!("PICUS_TEST_* not set — skipping.");
        return;
    }

    let outcome = probe().await;
    cleanup().await;
    let found = outcome.expect("the probe ran");

    // 1 — the relation tab does not fetch the payload.
    assert_eq!(found.tab.masked, vec!["allegato".to_string()], "the heavy column must be named");
    let reported: usize = found.tab.first.parse().expect("a size, not a value");
    assert!(
        (PAYLOAD_BYTES - 64..PAYLOAD_BYTES + 64).contains(&reported),
        "the size should be the payload's, got {reported}",
    );

    // 2 — a typed `SELECT *` is the same read, not a second behaviour.
    assert_eq!(found.typed.masked, found.tab.masked);
    assert_eq!(found.typed.first, found.tab.first);

    // 3 — naming the column changes nothing. THE regression this file exists for:
    //     `SELECT allegato FROM …` used to drag every byte across, because the
    //     masking recognised `SELECT *` and nothing else.
    assert_eq!(
        found.by_name.masked,
        vec!["allegato".to_string()],
        "naming the column must not be a way round the masking",
    );
    // PostgreSQL prints bytea as `\x…`; a size is digits. Asserted from the wrong
    // side too, because "it did not look like a hex dump" is the thing that would
    // still be true if the value came back some third way.
    assert!(!found.by_name.first.starts_with("\\x"), "the bytes must not come back");
    assert!(found.by_name.first.parse::<usize>().is_ok(), "a size, not a value");

    // 4 — no key, and masked all the same. Its cells cannot be opened, which is a
    //     smaller problem than a read nobody can wait out.
    assert_eq!(found.keyless.masked, vec!["allegato".to_string()]);
    assert!(found.keyless.first.parse::<usize>().is_ok());

    println!(
        "\n  tab {} ms · typed {} ms · by name {} ms",
        found.tab.millis, found.typed.millis, found.by_name.millis
    );
}
