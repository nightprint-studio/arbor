//! Driver error → [`DbError`] mapping.
//!
//! The point of mapping rather than stringifying: the frontend behaves differently
//! for a few of these. A read-only refusal offers to switch the connection; a lost
//! session offers to reconnect; a SQL error carries a position the editor can put a
//! squiggle at. Everything else is honestly generic.

use std::error::Error as StdError;

use picus_db_api::prelude::DbError;
use tokio_postgres::error::SqlState;

/// A driver error **with its cause**, as one line.
///
/// `tokio_postgres::Error`'s `Display` is a category, not a reason: every failure
/// to reach a server — the port closed, the host unresolvable, a TLS handshake
/// rejected, a certificate signed by an unknown authority — prints the same seven
/// words, `error connecting to server`, and the thing that would tell you which
/// sits one level down in `source()`.
///
/// So the chain is walked and joined. This is not tidiness: "cannot connect: error
/// connecting to server" is a message that sends someone to read the code, and
/// this product's whole claim is that its refusals say why. `to_string()` alone
/// was throwing the answer away at the last step.
///
/// Duplicate links are skipped — some wrappers already embed their source's text,
/// and repeating it reads as two different problems.
fn with_causes(err: &(dyn StdError + 'static)) -> String {
    let mut out = err.to_string();
    let mut cursor = err.source();
    while let Some(cause) = cursor {
        let text = cause.to_string();
        if !text.is_empty() && !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        cursor = cause.source();
    }
    out
}

/// Map a `tokio_postgres::Error` onto the provider contract.
pub fn map_pg(err: tokio_postgres::Error) -> DbError {
    // The database's own error carries the detail worth keeping; a driver-level
    // failure (socket closed, TLS handshake) carries its detail in the cause chain
    // rather than in its own message, so that is what is read.
    let Some(db) = err.as_db_error() else {
        return DbError::Disconnected(with_causes(&err));
    };

    let code = db.code();
    let message = db.message().to_string();

    if *code == SqlState::READ_ONLY_SQL_TRANSACTION {
        // The server refused a write on a read-only session — the enforcement
        // actually firing. Restate it in the product's words.
        return DbError::ReadOnly { statement: "a write".to_string() };
    }
    if *code == SqlState::INVALID_PASSWORD || *code == SqlState::INVALID_AUTHORIZATION_SPECIFICATION
    {
        return DbError::AuthFailed(message);
    }
    if *code == SqlState::UNDEFINED_TABLE
        || *code == SqlState::UNDEFINED_COLUMN
        || *code == SqlState::UNDEFINED_FUNCTION
        || *code == SqlState::UNDEFINED_OBJECT
    {
        return DbError::NotFound(message);
    }
    if *code == SqlState::QUERY_CANCELED {
        return DbError::Cancelled;
    }
    if *code == SqlState::ADMIN_SHUTDOWN || *code == SqlState::CRASH_SHUTDOWN {
        return DbError::Disconnected(message);
    }

    DbError::Sql {
        message,
        code: Some(code.code().to_string()),
        // `position` is 1-based and counts characters, which is exactly what the
        // editor wants to place a marker.
        position: db.position().and_then(position_offset),
    }
}

/// Extract the numeric offset from a `ErrorPosition`, which distinguishes a
/// position in the submitted statement from one in an internally-generated query
/// (a function body). Only the former can be pointed at in the editor.
fn position_offset(pos: &tokio_postgres::error::ErrorPosition) -> Option<u32> {
    match pos {
        tokio_postgres::error::ErrorPosition::Original(p) => Some(*p),
        // The offset refers to a query the user never wrote — pointing at it would
        // put the squiggle on an unrelated character.
        tokio_postgres::error::ErrorPosition::Internal { .. } => None,
    }
}
