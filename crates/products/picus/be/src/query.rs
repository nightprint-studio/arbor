//! `query` domain — running a statement and cancelling one.
//!
//! Cancellation is a separate handler on purpose, and it works because a session is
//! shared behind an `Arc`: the handler running the query and the one cancelling it
//! hold the same session at the same time. A cancel that had to wait for the query
//! handler to finish would not be a cancel.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::QueryResult;

use crate::connections::require_session;

/// Run a statement against an open connection.
///
/// `limit` caps the rows returned and the result says whether it truncated, so the
/// UI can report "first 500 of more" instead of letting the user believe they saw
/// everything.
///
/// On a read-only connection a write is refused **by the server** — the session was
/// opened in a read-only transaction mode. The provider's lexical check only makes
/// the refusal arrive sooner and in the product's own words.
#[arbor_rpc::handler]
async fn picus_execute(
    state: &PicusState,
    id: String,
    sql: String,
    limit: u32,
) -> Result<QueryResult, String> {
    require_session(state, &id)?.execute(&sql, limit).await.map_err(|e| e.to_string())
}

/// Ask the server to cancel whatever this connection is running.
///
/// A no-op when nothing is running. Never an error the user has to acknowledge:
/// pressing Cancel on an already-finished query should feel like nothing happened,
/// because nothing did.
#[arbor_rpc::handler]
async fn picus_cancel(state: &PicusState, id: String) -> Result<(), String> {
    let Some(session) = state.sessions().get(&id) else { return Ok(()) };
    session.cancel().await.map_err(|e| e.to_string())
}
