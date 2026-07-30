//! `schema` domain — reading what a live connection contains.
//!
//! Two granularities on purpose. [`picus_read_schema`] is the tree: every relation
//! with its columns, cheap enough for a schema with hundreds of tables.
//! [`picus_table_detail`] adds the constraints and indexes, and is only paid for
//! when a tab actually opens.
//!
//! A relation's *rows* are not here — they are a read like any other, and live with
//! the rest of the reads in [`crate::query`] (`picus_open_relation`). Structure and
//! data are different questions about the same object, and only one of them scrolls.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{SchemaSnapshot, TableInfo, TriggerDetail};

use crate::connections::require_session;

/// The whole schema of an open connection: tables, views, sequences, triggers.
///
/// The answer is also **held** ([`picus_core::prelude::SchemaCache`]), because one
/// caller asks constantly: the SQL abbreviation expander, on every keystroke. It
/// is stored here rather than fetched there so there is one place a schema is read
/// from a server, and re-reading — which is what this call is — replaces it.
#[arbor_rpc::handler]
async fn picus_read_schema(state: &PicusState, id: String) -> Result<SchemaSnapshot, String> {
    // Counted, and said out loud when there is more than one.
    //
    // Reading a catalogue is one round trip per connection and should never overlap
    // with itself. When it does — an interface effect re-firing while the first read
    // is still out — the queries pipeline down the one connection and each waits for
    // the ones before it, so the *last* caller sees a read that took as long as all
    // of them together. That looks exactly like a slow database and is not one, and
    // the difference is invisible unless somebody counts.
    static IN_FLIGHT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let concurrent = IN_FLIGHT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    if concurrent > 1 {
        eprintln!(
            "picus: {concurrent} schema reads of `{id}` are in flight at once — they queue on \
             one connection, so this one will take as long as all of them"
        );
    }
    let read = require_session(state, &id)?.read_schema().await;
    IN_FLIGHT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);

    let schema = read.map_err(|e| e.to_string())?;
    state.schemas().put(&id, std::sync::Arc::new(schema.clone()));
    Ok(schema)
}

/// One relation in full: columns, primary key, foreign keys, indexes — or, for a
/// view, its defining SELECT.
#[arbor_rpc::handler]
async fn picus_table_detail(
    state: &PicusState,
    id: String,
    name: String,
) -> Result<TableInfo, String> {
    require_session(state, &id)?.table_detail(&name).await.map_err(|e| e.to_string())
}

/// What a trigger does: its `CREATE TRIGGER` and the source of the routine it fires.
///
/// Asked when a trigger's tab opens, never as part of the schema — see
/// [`picus_db_api::prelude::DbSession::trigger_detail`] for why a routine body has
/// no business in a snapshot that is cached and handed over on connect.
#[arbor_rpc::handler]
async fn picus_trigger_detail(
    state: &PicusState,
    id: String,
    name: String,
) -> Result<TriggerDetail, String> {
    require_session(state, &id)?.trigger_detail(&name).await.map_err(|e| e.to_string())
}
