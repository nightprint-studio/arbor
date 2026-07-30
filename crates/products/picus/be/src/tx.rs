//! `tx` domain — explicit transactions on an open connection.
//!
//! Four verbs over the seam, and the shape of them is the point: `begin`, `commit`
//! and `rollback` each answer with the state the connection is **in afterwards**,
//! read back from the server, and [`picus_tx_state`] asks the same question on its
//! own. Nothing in this process remembers whether a transaction is open — the engine
//! is the only thing that knows, because it is the only thing that can abort one on
//! its own (a statement that fails inside a block puts PostgreSQL into an aborted
//! transaction that refuses everything until it is ended).
//!
//! The interface calls [`picus_tx_state`] after every statement for exactly that
//! reason: an open transaction changes what the *next* statement means, and a failed
//! one changes whether there will be a next statement at all. Both facts have to be
//! on screen before the user writes it, not after.
//!
//! An engine without transactions answers
//! [`DbError::unsupported`](picus_db_api::prelude::DbError::unsupported) from the
//! trait's own default — but the interface should never get that far: it reads
//! `capabilities.transactions.supported` from the descriptor and does not offer what
//! the engine does not have.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{TxOutcome, TxState};

use crate::connections::require_session;

/// Open an explicit transaction. Fails, in words, when one is already open.
#[arbor_rpc::handler]
async fn picus_tx_begin(state: &PicusState, id: String) -> Result<TxOutcome, String> {
    require_session(state, &id)?.begin().await.map_err(|e| e.to_string())
}

/// Commit the open transaction.
///
/// Refused on a failed one rather than forwarded, and that refusal is the engine's
/// to explain: on PostgreSQL a `COMMIT` inside an aborted block *performs a
/// rollback* and reports success, so passing it through would tell the user their
/// work was saved at the moment it was discarded.
#[arbor_rpc::handler]
async fn picus_tx_commit(state: &PicusState, id: String) -> Result<TxOutcome, String> {
    require_session(state, &id)?.commit().await.map_err(|e| e.to_string())
}

/// Roll the open transaction back.
///
/// The one call here that must work in every state the others refuse — a failed
/// transaction is precisely the situation where it is the only thing left. Rolling
/// back when nothing is open is success with a message saying so, not an error:
/// this is also the call the interface makes on the way out of a window, and a
/// close that fails because there was nothing to undo would be a close nobody
/// completes.
#[arbor_rpc::handler]
async fn picus_tx_rollback(state: &PicusState, id: String) -> Result<TxOutcome, String> {
    require_session(state, &id)?.rollback().await.map_err(|e| e.to_string())
}

/// Where the transaction stands, asked of the server.
#[arbor_rpc::handler]
async fn picus_tx_state(state: &PicusState, id: String) -> Result<TxState, String> {
    require_session(state, &id)?.tx_state().await.map_err(|e| e.to_string())
}
