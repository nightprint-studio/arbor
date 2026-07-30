//! `query` domain — running a statement, scrolling its result, and cancelling
//! either.
//!
//! ## One door for every statement
//!
//! [`picus_execute`] takes any SQL. A read comes back as a **held result** —
//! `resultId` plus its first window — and every later window is
//! [`picus_result_window`] against that id; a write comes back with `affected` and
//! `resultId: null`. The frontend never has to classify SQL, which is the point:
//! deciding what a statement is takes a dialect-aware scan, and a caller that gets
//! it wrong gets it wrong silently.
//!
//! ## The two numbers a scrollbar needs
//!
//! `estimatedRows` arrives with the first window (the planner's guess — the UI shows
//! it with a `~`); the exact figure comes from [`picus_count_result`], which the
//! caller asks for in the background and which replaces the estimate when it lands.
//! Splitting them is not an optimisation: an exact count over a large result takes
//! seconds, and nothing may stand between the user and their first rows.
//!
//! ## Field names
//!
//! The wire keys are `connectionId` / `resultId`, so the handler arguments are
//! spelled that way — `#[handler]` decodes each argument by its own identifier, and
//! the wire contract wins over the naming convention. Hence the module-wide allow;
//! it is the only reason for it.
#![allow(non_snake_case)]

use picus_ast::prelude::DialectScope;
use picus_core::prelude::PicusState;
use picus_db_api::prelude::{
    CellValue, ExecuteResult, ResultCount, ResultWindow, DEFAULT_WINDOW_ROWS,
};
use picus_emit::prelude::{ident, literal};
use serde::Serialize;

use crate::connections::require_session;

/// Run a statement against an open connection.
///
/// For a read this opens a cursor the server holds over one snapshot and returns
/// its first window; `resultId` names it, and the caller owns closing it (see
/// [`picus_close_result`]). For a write or a session statement nothing is held and
/// `affected` says what changed.
///
/// On a read-only connection a write is refused **by the server** — the session was
/// opened in a read-only transaction mode. The provider's lexical check only makes
/// the refusal arrive sooner and in the product's own words.
#[arbor_rpc::handler]
async fn picus_execute(
    state: &PicusState,
    connectionId: String,
    sql: String,
    window: Option<u32>,
) -> Result<ExecuteResult, String> {
    require_session(state, &connectionId)?
        .execute(&sql, window_size(window))
        .await
        .map_err(|e| e.to_string())
}

/// How many rows the **first** window holds.
///
/// Optional so an omitted key means "whatever the backend thinks", but the caller
/// should send it: the user's own "rows per window" setting governs every later
/// window, and a first window of a different size than all the others is the kind
/// of inconsistency nobody reports as a bug and everybody notices.
///
/// Clamped rather than trusted — a hand-edited `0` would ask the server for an
/// empty window and read as a result with no rows at all.
pub(crate) fn window_size(requested: Option<u32>) -> u32 {
    requested.filter(|n| *n > 0).unwrap_or(DEFAULT_WINDOW_ROWS)
}

/// Open a relation's data — the table tab's Data view.
///
/// Deliberately in this module rather than with the schema reads: opening a
/// relation *is* a read, and it goes down the same path as a typed query, holds the
/// same kind of result and is closed the same way. Making it a separate mechanism is
/// how a product ends up with two scrolling behaviours.
///
/// The relation name is quoted by the engine. Identifier quoting is engine-specific
/// and belongs there — a caller composing `"public"."Orders"` would be writing one
/// dialect's syntax for all of them.
#[arbor_rpc::handler]
async fn picus_open_relation(
    state: &PicusState,
    connectionId: String,
    relation: String,
    window: Option<u32>,
) -> Result<ExecuteResult, String> {
    require_session(state, &connectionId)?
        .open_relation(&relation, window_size(window))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_or_nonsensical_window_falls_back_to_the_default() {
        assert_eq!(window_size(Some(250)), 250);
        assert_eq!(window_size(None), DEFAULT_WINDOW_ROWS);
        // A hand-edited `0` would ask the server for nothing and read as an empty
        // result, which is indistinguishable from a query that matched no rows.
        assert_eq!(window_size(Some(0)), DEFAULT_WINDOW_ROWS);
    }
}

/// One window over a held result — any offset, forwards or backwards.
///
/// `offset` is echoed back in the reply. That is load-bearing rather than tidy: a
/// scrolling grid has several of these in flight and they do not necessarily come
/// back in order, so the caller must be able to tell which question an answer
/// belongs to before it paints anything.
#[arbor_rpc::handler]
async fn picus_result_window(
    state: &PicusState,
    connectionId: String,
    resultId: String,
    offset: u64,
    limit: u32,
) -> Result<ResultWindow, String> {
    require_session(state, &connectionId)?
        .result_window(&resultId, offset, limit)
        .await
        .map_err(|e| e.to_string())
}

/// The exact number of rows in a held result.
///
/// Long-running by nature, and cancelled the same way any statement is: this takes a
/// run ordinal on the session, so [`picus_cancel`] on the same connection interrupts
/// it. Note the consequence of a session being one connection — a cancel aimed at
/// the count stops whatever that connection is running at that instant, so a caller
/// should not fire one while a query it still wants is in flight.
#[arbor_rpc::handler]
async fn picus_count_result(
    state: &PicusState,
    connectionId: String,
    resultId: String,
) -> Result<ResultCount, String> {
    require_session(state, &connectionId)?
        .count_result(&resultId)
        .await
        .map_err(|e| e.to_string())
}

/// Release a held result.
///
/// Idempotent, and idempotent all the way down: an unknown id, a result the engine
/// already reclaimed, a connection that is no longer open — all of them are success.
/// A close that can fail is a close callers start skipping, and a cursor nobody
/// closes is storage held on somebody's production database.
#[arbor_rpc::handler]
async fn picus_close_result(
    state: &PicusState,
    connectionId: String,
    resultId: String,
) -> Result<(), String> {
    let Some(session) = state.sessions().get(&connectionId) else { return Ok(()) };
    session.close_result(&resultId).await.map_err(|e| e.to_string())
}

/// Ask the server to cancel whatever this connection is running.
///
/// A no-op when nothing is running. Never an error the user has to acknowledge:
/// pressing Cancel on an already-finished query should feel like nothing happened,
/// because nothing did.
///
/// Its parameter stays `id`, unlike its neighbours above. Not an oversight: this
/// method's signature is unchanged by the scrolling work, and renaming a live
/// parameter that nothing asked to be renamed breaks every existing caller for
/// tidiness. The rest of the picus surface (`picus_read_schema`,
/// `picus_table_detail`, the connection calls) uses `id` for the same reason.
#[arbor_rpc::handler]
async fn picus_cancel(state: &PicusState, id: String) -> Result<(), String> {
    let Some(session) = state.sessions().get(&id) else { return Ok(()) };
    session.cancel().await.map_err(|e| e.to_string())
}

// ── Large objects, read one at a time ────────────────────────────────────────

/// The largest value read into the interface at once.
///
/// A cap and not a preference: a 900 MB blob is a legitimate thing to have in a
/// column and an illegitimate thing to put in a webview. Past this the value is
/// truncated and **says so**, so the panel never claims to be showing all of it.
const LOB_LIMIT: u64 = 4 * 1024 * 1024;

/// One large object, read on demand.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobValue {
    /// The whole value's size in bytes, which may exceed what came back.
    pub bytes: u64,
    /// Set when the column holds text — the value itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Set when the column holds bytes — the value, base64-encoded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
    /// The value was longer than [`LOB_LIMIT`] and only its beginning is here.
    pub truncated: bool,
}

/// Read one large object: the value a masked cell stands for.
///
/// The counterpart of the masking a relation read applies. Two things make it safe
/// to do at all:
///
/// * **it is one value.** The row is addressed by its key, so exactly the cell the
///   user clicked is fetched — not a column, and not a window;
/// * **it is capped.** Past [`LOB_LIMIT`] the answer is truncated and marked, so
///   clicking a cell can never be the thing that fills the window's memory.
///
/// Binary comes back base64-encoded rather than as hex or as raw bytes: it crosses a
/// JSON seam, it has to survive being a string, and base64 is a third smaller than
/// hex for a value that is measured in megabytes.
#[arbor_rpc::handler]
async fn picus_read_lob(
    state: &PicusState,
    id: String,
    table: String,
    keys: std::collections::BTreeMap<String, Option<String>>,
    column: String,
) -> Result<LobValue, String> {
    if keys.is_empty() {
        return Err(NO_KEY_FOR_LOB.to_string());
    }
    let spec = crate::connections::find_spec(&id)?;
    let session = require_session(state, &id)?;
    let schema =
        state.schemas().get(&id).ok_or("the schema of this connection has not been read yet")?;
    let info = schema
        .tables
        .iter()
        .chain(schema.views.iter())
        .find(|t| t.name.eq_ignore_ascii_case(&table))
        .ok_or_else(|| format!("{table} is not a relation on this connection"))?;

    let described = |name: &str| {
        info.columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .cloned()
            .ok_or_else(|| format!("{} has no column {name}", info.name))
    };
    let target = described(&column)?;
    let textual = matches!(
        target.data_type.trim().to_ascii_lowercase().as_str(),
        "clob" | "nclob" | "text" | "xml"
    );

    let scope = DialectScope::One(spec.engine);
    let id_of = |name: &str| ident(name, scope, false);
    let filter = keys
        .iter()
        .map(|(name, value)| {
            let col = described(name)?;
            Ok(format!("{} = {}", id_of(name), literal(value.as_deref(), &col, scope)))
        })
        .collect::<Result<Vec<_>, String>>()?
        .join(" AND ");

    // `octet_length` on the value itself, and the *encoded* slice beside it: the
    // size reported is the whole value's, so a truncated read still says how much
    // there is rather than how much arrived.
    let projection = if textual {
        format!(
            "octet_length({c}) AS \"n\", substr({c}, 1, {LOB_LIMIT}) AS \"v\"",
            c = id_of(&column)
        )
    } else {
        format!(
            "octet_length({c}) AS \"n\", encode(substr({c}, 1, {LOB_LIMIT}), 'base64') AS \"v\"",
            c = id_of(&column)
        )
    };
    let sql = format!("SELECT {projection} FROM {} WHERE {filter}", id_of(&info.name));

    let result = session.execute(&sql, 1).await.map_err(|e| e.to_string())?;
    // A read goes down the cursor path like any other, so this one-row lookup left a
    // held result behind — up to `LOB_LIMIT` of tuplestore on the server, per cell
    // opened, until the idle sweep got to it. Nothing is going to scroll a single
    // value, so it is released now.
    if let Some(held) = &result.result_id {
        let _ = session.close_result(held).await;
    }
    let row = result.rows.first().ok_or(LOB_ROW_GONE)?;
    let cell = |at: usize| match row.get(at) {
        Some(CellValue::Text(text)) => Some(text.clone()),
        Some(CellValue::Int(n)) => Some(n.to_string()),
        Some(CellValue::Float(n)) => Some(n.to_string()),
        _ => None,
    };
    let bytes = cell(0).and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
    let value = cell(1);

    Ok(LobValue {
        bytes,
        truncated: bytes > LOB_LIMIT,
        text: textual.then_some(value.clone()).flatten(),
        base64: (!textual).then_some(value).flatten(),
    })
}

const NO_KEY_FOR_LOB: &str = "there is no key to read this value by — a large object is fetched \
    one row at a time, and that needs something that identifies the row";

const LOB_ROW_GONE: &str = "that row is no longer there. Re-run the query to see the current \
    state of the table.";
