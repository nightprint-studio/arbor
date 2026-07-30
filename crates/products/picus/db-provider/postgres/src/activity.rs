//! The session monitor's two reads: what every backend is doing, and asking one
//! of them to stop.
//!
//! ## One statement, not three
//!
//! `pg_stat_activity`, the blocking graph and the lock being waited for are read
//! **together**, in a single round trip. Splitting them is the obvious shape and it
//! is wrong: `pg_stat_activity` is a snapshot taken when the view is scanned, so a
//! second query to `pg_locks` describes a moment that has already moved on, and the
//! interface ends up drawing an arrow from a session to one that finished in
//! between. `pg_blocking_pids()` is a function, so it can travel in the projection
//! and the whole picture comes from one scan.
//!
//! ## The clock is the server's, always
//!
//! Every age is computed by the server as milliseconds and carried as a number.
//! Sending the timestamps and subtracting them from the browser's clock would be
//! wrong by whatever the two machines disagree by — routinely minutes, occasionally
//! hours on a server in another timezone whose NTP has drifted — and wrong in the
//! way nobody notices, because "this query has been running for 3 hours" looks like
//! a finding rather than like a bug.
//!
//! `clock_timestamp()` rather than `now()`: `now()` is the *transaction's* start
//! time, and while that is a few milliseconds ago for this read, using it here
//! would make the ages quietly wrong the day this is called inside a longer
//! transaction.
//!
//! ## Nothing here may return NULL into a non-nullable field
//!
//! Same rule as [`crate::catalog`], and for the same reason — `Row::get` panics
//! rather than failing. Half of `pg_stat_activity` is nullable (an idle backend has
//! no `query_start`, a local connection has no `client_addr`, a background worker
//! has no `usename`), so every column either coalesces in the SQL or is decoded
//! into an `Option`.

use picus_db_api::prelude::*;
use tokio_postgres::Client;

use crate::error::map_pg;

/// Every client backend, its ages, and who it is waiting for.
///
/// Columns are read **by name**. The house style elsewhere is positional, and that
/// is fine for a query of five columns; this one projects sixteen expressions, and
/// an index quietly off by one there is a wrong pid next to a wrong age — the one
/// class of mistake this panel exists to not make.
///
/// `backend_type = 'client backend'` leaves out the checkpointer, the autovacuum
/// launcher and the walwriter. They are sessions in the view's sense and nothing a
/// person can act on: they cannot be cancelled, they never block anybody, and
/// listing them puts six rows nobody reads above the one that matters.
///
/// Not restricted to the current database, deliberately. The question the monitor
/// answers is "what is this *server* doing" — a lock held from another database on
/// a shared catalogue is exactly the kind of thing that is invisible until someone
/// looks for it.
const ACTIVITY_SQL: &str = "
    SELECT a.pid                                        AS pid,
           COALESCE(a.usename::text, '')                AS usename,
           COALESCE(a.datname::text, '')                AS datname,
           COALESCE(a.application_name, '')             AS application_name,
           -- `host()` prints the address without the mask `inet` carries; NULL is a
           -- connection over the local socket, which has no address at all rather
           -- than an unknown one.
           COALESCE(host(a.client_addr), '')            AS client_addr,
           COALESCE(a.state, '')                        AS state,
           a.wait_event_type                            AS wait_event_type,
           a.wait_event                                 AS wait_event,
           COALESCE(a.query, '')                        AS query,
           (extract(epoch from (clock_timestamp() - a.query_start))  * 1000)::bigint AS query_age_ms,
           (extract(epoch from (clock_timestamp() - a.state_change)) * 1000)::bigint AS state_age_ms,
           (extract(epoch from (clock_timestamp() - a.xact_start))   * 1000)::bigint AS xact_age_ms,
           (a.pid = pg_backend_pid())                   AS is_self,
           -- The graph, resolved by the server. Doing it from `pg_locks` by hand
           -- means re-implementing what counts as a conflict, and getting it right
           -- for every lock type there is.
           COALESCE(pg_blocking_pids(a.pid), '{}')      AS blocked_by,
           w.relation                                   AS relation,
           w.mode                                       AS mode,
           to_char(clock_timestamp(), 'YYYY-MM-DD HH24:MI:SS') AS read_at
      FROM pg_stat_activity a
      -- What this backend is waiting FOR, when it is waiting. One ungranted lock is
      -- enough: a backend waits on exactly one at a time, and `LIMIT 1` keeps the
      -- lateral from multiplying the row if that ever stops being true.
      LEFT JOIN LATERAL (
              SELECT COALESCE(c.relname::text, l.locktype) AS relation, l.mode AS mode
                FROM pg_locks l
                -- A relation OID only means something in the database that holds
                -- it. Without this the same number resolves to whatever table
                -- happens to wear it here, and the panel names the wrong object
                -- with complete confidence.
           LEFT JOIN pg_class c
                  ON c.oid = l.relation
                 AND l.database = (SELECT d.oid FROM pg_database d
                                    WHERE d.datname = current_database())
               WHERE l.pid = a.pid AND NOT l.granted
               LIMIT 1
           ) w ON true
     WHERE a.backend_type = 'client backend'";

/// One coherent picture of the server: every session, and the blocking graph.
pub async fn read_activity(client: &Client) -> DbResult<ActivitySnapshot> {
    let rows = client.query(ACTIVITY_SQL, &[]).await.map_err(map_pg)?;

    // The server's own formatting of the instant it answered. Taken off the first
    // row because it is the same value on all of them; empty only if the view came
    // back with nothing, which cannot happen while we are connected — this session
    // is one of the rows.
    let read_at: String = rows.first().map(|r| r.get("read_at")).unwrap_or_default();

    let mut sessions = Vec::with_capacity(rows.len());
    let mut blocked = Vec::new();

    for row in &rows {
        let pid: i32 = row.get("pid");
        let blockers: Option<Vec<i32>> = row.get("blocked_by");
        let blocked_by = blockers.unwrap_or_default();
        let relation: Option<String> = row.get("relation");
        let mode: Option<String> = row.get("mode");

        // One edge per blocker: a session can be behind several holders of the same
        // lock, and collapsing them to "blocked" loses the only thing that makes the
        // chain walkable back to its root.
        for blocker in &blocked_by {
            blocked.push(BlockEdge {
                waiter: pid,
                blocker: *blocker,
                relation: relation.clone(),
                mode: mode.clone(),
            });
        }

        sessions.push(SessionActivity {
            pid,
            user: row.get("usename"),
            database: row.get("datname"),
            application: row.get("application_name"),
            client: row.get("client_addr"),
            state: row.get("state"),
            wait_event: wait_label(row.get("wait_event_type"), row.get("wait_event")),
            query: row.get("query"),
            query_age_ms: row.get("query_age_ms"),
            state_age_ms: row.get("state_age_ms"),
            transaction_age_ms: row.get("xact_age_ms"),
            is_self: row.get("is_self"),
            blocked_by,
        });
    }

    // By pid, so a refresh three seconds later does not reshuffle rows under the
    // pointer. Anything more opinionated — blocked first, oldest first — is a
    // display decision and belongs where the display is.
    sessions.sort_by_key(|s| s.pid);
    Ok(ActivitySnapshot { sessions, blocked, read_at })
}

/// `Lock: transactionid` — the wait type and the wait, as one readable phrase.
///
/// The type alone is a category and the event alone is ambiguous (`transactionid`
/// is a lock, `DataFileRead` is I/O, and both are just words without it). `None`
/// when the backend is not waiting, which is what "it is running" looks like here.
fn wait_label(kind: Option<String>, event: Option<String>) -> Option<String> {
    let event = event?;
    Some(match kind {
        Some(kind) => format!("{kind}: {event}"),
        None => event,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    #[test]
    fn a_wait_is_named_by_its_type_and_its_event() {
        assert_eq!(wait_label(s("Lock"), s("transactionid")).as_deref(), Some("Lock: transactionid"));
    }

    #[test]
    fn no_event_means_the_backend_is_not_waiting() {
        // The type without the event is what a running backend looks like in some
        // server versions, and "Lock" on its own would read as "it is blocked".
        assert_eq!(wait_label(s("Lock"), None), None);
        assert_eq!(wait_label(None, None), None);
    }

    #[test]
    fn an_event_with_no_type_still_says_what_it_is_waiting_on() {
        assert_eq!(wait_label(None, s("DataFileRead")).as_deref(), Some("DataFileRead"));
    }
}

/// Ask one backend to stop, and report what the server said.
///
/// The return value is the server's own boolean, untouched: `false` means it did
/// not find that pid — the session ended between the read and the click, which is
/// ordinary and is not an error. A refusal for want of privilege is *not* this
/// case; PostgreSQL raises for that, and the raise travels out of here with the
/// server's own words ("permission denied to terminate process", "must be a member
/// of the role…"). That distinction is the whole reason this returns a bool and an
/// error rather than one merged verdict — a silent no-op here reads to the user as
/// "Picus's Terminate button does nothing".
pub async fn stop_session(client: &Client, pid: i32, kind: StopKind) -> DbResult<bool> {
    // Cancel interrupts the running statement and leaves the connection up;
    // terminate closes it and rolls its transaction back. The caller decided which;
    // this only carries the decision to the right function.
    const CANCEL: &str = "SELECT pg_cancel_backend($1)";
    const TERMINATE: &str = "SELECT pg_terminate_backend($1)";

    let sql = match kind {
        StopKind::Cancel => CANCEL,
        StopKind::Terminate => TERMINATE,
    };
    let row = client.query_one(sql, &[&pid]).await.map_err(map_pg)?;
    // `Option<bool>`: the functions are declared to return `boolean` and in practice
    // never NULL, but a NULL decoded into `bool` is a panic rather than an error —
    // and this crate has paid for that lesson once already (see `catalog`).
    let stopped: Option<bool> = row.get(0);
    Ok(stopped.unwrap_or(false))
}
