//! Explicit transactions on a PostgreSQL session — `BEGIN`, then a decision that
//! belongs to the user rather than to the driver.
//!
//! Four operations, no state of our own. That second half is the design, and it is
//! worth stating before the code: **nothing here remembers whether a transaction is
//! open.** A boolean set by `begin` and cleared by `commit` is right until the
//! server disagrees — and the server disagrees routinely, because a statement that
//! fails inside a transaction block puts PostgreSQL into an *aborted* one, where
//! every later statement is refused until something ends it. A client flag would
//! still be saying "active" while the connection accepted nothing, the interface
//! would still be offering a Commit that silently throws the work away (see
//! [`commit`]), and the user would find out afterwards. So every call here asks.
//!
//! ## Where the state comes from
//!
//! PostgreSQL puts the answer in the protocol: every `ReadyForQuery` frame carries a
//! transaction-status byte — `I` idle, `T` in a block, `E` aborted — which is what
//! `psql` reads. **`tokio-postgres` does not expose it** (0.7.18 parses the frame
//! and discards the byte; `Client` offers `is_closed`, `cancel_token`,
//! `transaction()`, and nothing else about it), and this crate does not fork a
//! driver for one byte.
//!
//! So it is deduced, with one round trip and no side effects, from [`PROBE`]:
//!
//! * the probe **fails with `25P02`** → the block is aborted → [`TxState::Failed`];
//! * it answers **true** → `transaction_timestamp()` predates this statement, so the
//!   transaction began at an earlier statement → [`TxState::Active`];
//! * it answers **false** → the two stamps coincide, which is what an implicit
//!   single-statement transaction looks like → [`TxState::None`].
//!
//! The one assumption, stated plainly: a `BEGIN` and the probe are separate round
//! trips, so their timestamps cannot coincide. PostgreSQL stamps to the microsecond
//! and a round trip is orders of magnitude longer than that, including over a local
//! socket.
//!
//! [`TxState::Savepoint`] is never returned here, and that is not an omission: the
//! server's own transaction status does not distinguish a block with savepoints from
//! one without, so claiming it would be a guess. A session that issues savepoints
//! itself is the thing that knows.

use picus_db_api::prelude::{DbError, DbResult, TxOutcome, TxState};
use tokio_postgres::{Client, SimpleQueryMessage};

use crate::error::map_pg;

/// The question, asked in a way that changes nothing.
///
/// `transaction_timestamp()` is fixed at the start of the transaction;
/// `statement_timestamp()` moves with every statement. Inside a block opened by an
/// earlier statement they differ; outside one — where the statement *is* the whole
/// transaction — they are the same value.
///
/// Schema-qualified deliberately: the session's `search_path` is the user's, and a
/// probe whose meaning depends on it is not a probe. It reads no table and takes no
/// lock, so it is safe on a read-only connection and safe to ask after every
/// statement.
const PROBE: &str =
    "SELECT pg_catalog.transaction_timestamp() <> pg_catalog.statement_timestamp()";

/// PostgreSQL's SQLSTATE for "current transaction is aborted, commands ignored
/// until end of transaction block" — the *only* thing an aborted block answers.
const IN_FAILED_TRANSACTION: &str = "25P02";

/// Open an explicit transaction.
///
/// Refuses when one is already open. PostgreSQL would not: a second `BEGIN` is a
/// **warning** there (`there is already a transaction in progress`) and the
/// statement succeeds having done nothing, which is exactly the shape of an
/// interface that lies — the button lit up, the banner appeared, and the
/// transaction the user believes they just started is somebody else's tab's, minutes
/// old.
pub async fn begin(client: &Client) -> DbResult<TxOutcome> {
    match state(client).await? {
        TxState::None => {}
        TxState::Failed => return Err(refusal(ALREADY_FAILED)),
        TxState::Active | TxState::Savepoint => return Err(refusal(ALREADY_OPEN)),
    }
    client.simple_query("BEGIN").await.map_err(map_pg)?;
    // Read back rather than assumed. It costs one round trip on an operation the
    // user performs by hand, and it is the difference between reporting what the
    // server did and reporting what was asked of it.
    Ok(outcome(state(client).await?, OPENED))
}

/// Commit the open transaction.
///
/// ## Why a failed transaction is refused instead of forwarded
///
/// `COMMIT` on an aborted block is accepted by PostgreSQL, and what it performs is a
/// **rollback** — the command tag that comes back is literally `ROLLBACK`. Passing
/// that through would mean the user pressing Commit, seeing a success, and having
/// lost every statement in the transaction. The refusal here is the honest form of
/// the same fact: the work is already gone, and the only operation left is the one
/// that says so.
pub async fn commit(client: &Client) -> DbResult<TxOutcome> {
    match state(client).await? {
        TxState::None => return Err(refusal(NOTHING_TO_COMMIT)),
        TxState::Failed => return Err(refusal(FAILED_CANNOT_COMMIT)),
        TxState::Active | TxState::Savepoint => {}
    }
    client.simple_query("COMMIT").await.map_err(map_pg)?;
    Ok(outcome(state(client).await?, COMMITTED))
}

/// Roll the open transaction back — **including, and especially, a failed one**.
///
/// The one operation that must not be gated on anything. A failed transaction is the
/// state where nothing else on the connection works, so a rollback that first asked
/// permission and then declined to act would leave the user with a connection that
/// refuses every statement and a button that refuses to fix it.
///
/// The probe is therefore for the *message* only: it is what lets the answer
/// distinguish "the failed transaction is over" from "there was nothing to undo",
/// and a probe that cannot be read stops none of it.
pub async fn rollback(client: &Client) -> DbResult<TxOutcome> {
    let before = state(client).await.ok();
    client.simple_query("ROLLBACK").await.map_err(map_pg)?;
    let message = match before {
        Some(TxState::None) => NOTHING_TO_ROLL_BACK,
        Some(TxState::Failed) => ROLLED_BACK_AFTER_FAILURE,
        _ => ROLLED_BACK,
    };
    Ok(outcome(state(client).await?, message))
}

/// Where this session's transaction stands, as the **server** has it.
///
/// Cheap enough to ask after every statement, which is what the interface does: a
/// transaction that failed two statements ago changes the meaning of the next one,
/// and the user has to be told before they write it rather than after.
pub async fn state(client: &Client) -> DbResult<TxState> {
    match client.simple_query(PROBE).await {
        Ok(messages) => Ok(state_of(first_cell(&messages))),
        Err(e) => {
            let mapped = map_pg(e);
            // An aborted block refuses the probe like it refuses everything else.
            // That refusal IS the answer — the one state the probe cannot report by
            // succeeding.
            if in_failed_transaction(&mapped) {
                return Ok(TxState::Failed);
            }
            Err(mapped)
        }
    }
}

/// The first cell of the first row, as the server printed it.
fn first_cell(messages: &[SimpleQueryMessage]) -> Option<&str> {
    messages.iter().find_map(|m| match m {
        SimpleQueryMessage::Row(row) => row.get(0),
        _ => None,
    })
}

/// The state a probe answer stands for.
///
/// `f` is the only "not in a block" answer PostgreSQL can give — the probe compares
/// two values that are never null — so everything else resolves to `Active`. That
/// asymmetry is deliberate rather than sloppy: an interface offering a Commit nobody
/// needs costs a click, and one that hides an open transaction lets somebody shut
/// the window on work they believe was saved.
fn state_of(answer: Option<&str>) -> TxState {
    match answer {
        Some("f") => TxState::None,
        _ => TxState::Active,
    }
}

/// Is this the server saying the transaction block is aborted?
fn in_failed_transaction(err: &DbError) -> bool {
    matches!(err, DbError::Sql { code: Some(code), .. } if code == IN_FAILED_TRANSACTION)
}

/// A refusal the user reads as a sentence.
///
/// [`DbError::Sql`] rather than [`DbError::Internal`] because of how these cross the
/// Model-D seam: the error's `Display` *is* the contract, and `Internal` prefixes
/// "internal error:" — which is a lie about a refusal the product made on purpose,
/// and reads to the user as a bug worth reporting.
fn refusal(message: &str) -> DbError {
    DbError::Sql { message: message.to_string(), code: None, position: None }
}

/// The reply to a transaction operation.
///
/// `statements` is **0 from here, always**, and it is worth being explicit about why
/// rather than inventing a number: the count is "statements that ran inside the
/// transaction that just ended", and these functions see one `Client` and no
/// statements at all. Only the session issuing them can count them. A caller that
/// wires that up fills the field in; until then a zero must render as "not counted"
/// rather than as "none ran".
fn outcome(state: TxState, message: &str) -> TxOutcome {
    TxOutcome { state, message: message.to_string(), statements: 0 }
}

// ── What the user reads ────────────────────────────────────────────────────────
//
// These strings cross the seam as `Display` and are shown verbatim. Written for the
// person who has just pressed a button and needs to know what to do next.

const ALREADY_OPEN: &str = "a transaction is already open on this connection — commit it or roll \
    it back before starting another";

const ALREADY_FAILED: &str = "the transaction on this connection has failed: PostgreSQL will \
    accept nothing further on it until it is rolled back. Roll back, then begin again.";

const NOTHING_TO_COMMIT: &str = "there is no open transaction to commit — statements on this \
    connection are taking effect as they run";

const FAILED_CANNOT_COMMIT: &str = "this transaction has failed and cannot be committed: \
    PostgreSQL would discard every statement in it and still report success. Rolling back is the \
    only thing that can honestly be done here.";

const OPENED: &str = "transaction open — nothing takes effect on this connection until you commit";

const COMMITTED: &str = "committed";

const ROLLED_BACK: &str = "rolled back — every statement in the transaction was undone";

const ROLLED_BACK_AFTER_FAILURE: &str = "rolled back — the failed transaction is over and the \
    connection accepts statements again";

const NOTHING_TO_ROLL_BACK: &str = "there was no open transaction — nothing to undo";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_probe_reads_the_server_and_touches_nothing() {
        // Schema-qualified: the session's `search_path` is the user's, and a probe
        // whose meaning depends on it answers a different question per connection.
        assert!(PROBE.contains("pg_catalog.transaction_timestamp()"));
        assert!(PROBE.contains("pg_catalog.statement_timestamp()"));
        // No table, no lock, no write — it must be safe on a read-only session and
        // safe to repeat after every statement.
        assert!(!PROBE.to_ascii_uppercase().contains(" FROM "));
    }

    #[test]
    fn only_an_explicit_false_means_no_transaction() {
        assert_eq!(state_of(Some("f")), TxState::None);
        assert_eq!(state_of(Some("t")), TxState::Active);
        // Anything unreadable errs toward "open". Hiding an open transaction is how
        // somebody closes the window on work they believe was saved; a spurious
        // Commit button costs a click.
        assert_eq!(state_of(None), TxState::Active);
        assert_eq!(state_of(Some("")), TxState::Active);
    }

    #[test]
    fn an_aborted_block_is_recognised_by_its_sqlstate_alone() {
        let aborted = DbError::Sql {
            message: "current transaction is aborted".to_string(),
            code: Some(IN_FAILED_TRANSACTION.to_string()),
            position: None,
        };
        assert!(in_failed_transaction(&aborted));

        // A different SQL error is a different fact: the statement failed, and
        // whether that aborted a block is what the next probe answers.
        let syntax = DbError::Sql {
            message: "syntax error".to_string(),
            code: Some("42601".to_string()),
            position: Some(1),
        };
        assert!(!in_failed_transaction(&syntax));
        // …and a lost connection must never be reported as a failed transaction:
        // the remedy for one is a rollback, for the other a reconnect.
        assert!(!in_failed_transaction(&DbError::Disconnected("socket closed".to_string())));
        assert!(!in_failed_transaction(&DbError::Cancelled));
    }

    #[test]
    fn a_refusal_reads_as_a_sentence_rather_than_as_a_bug() {
        // `Internal` would prefix "internal error:" — a lie about a refusal the
        // product made deliberately, and these strings are shown verbatim.
        assert_eq!(refusal(ALREADY_OPEN).to_string(), ALREADY_OPEN);
        assert!(!refusal(NOTHING_TO_COMMIT).to_string().contains("internal"));
    }

    #[test]
    fn the_commit_refusal_says_what_would_have_happened() {
        // The whole reason a failed transaction is refused rather than forwarded:
        // PostgreSQL's own COMMIT there performs a rollback and reports success.
        assert!(FAILED_CANNOT_COMMIT.contains("discard"));
        assert!(FAILED_CANNOT_COMMIT.contains("Rolling back"));
    }

    #[test]
    fn an_outcome_never_invents_a_statement_count() {
        // Zero means "not counted": these functions see a `Client`, not the session
        // that ran the statements.
        assert_eq!(outcome(TxState::Active, OPENED).statements, 0);
        assert_eq!(outcome(TxState::Active, OPENED).message, OPENED);
    }
}
