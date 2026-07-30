//! `plan` domain — what the server would do with a statement, and what it did.
//!
//! One handler, and the whole design of it is the distinction between the two:
//! `EXPLAIN` describes a statement, `EXPLAIN ANALYZE` **executes** it. The caller
//! asks for the second explicitly, the engine refuses it for anything that is not a
//! read, and the answer says which of the two it is ([`QueryPlan::analyzed`]) so the
//! interface can never label an estimate as a measurement.
//!
//! ## Field names
//!
//! `connectionId` on the wire, so the argument is spelled that way — `#[handler]`
//! decodes each argument by its own identifier and the wire contract wins over the
//! naming convention. That is the only reason for the module-wide allow.
#![allow(non_snake_case)]

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{PlanRequest, QueryPlan};

use crate::connections::require_session;

/// The plan for one statement.
///
/// `analyze` defaults to **false** when the key is absent, and that default is
/// load-bearing rather than tidy: an omitted flag must never be the one that runs
/// somebody's `DELETE`. The refusal for a non-read lives in the engine, where a
/// caller cannot go round it.
///
/// Whether the engine can answer at all is a capability the frontend reads from the
/// descriptor (`capabilities.explain`) rather than discovering here — a feature an
/// engine lacks should be *absent* from the interface, not present and failing.
#[arbor_rpc::handler]
async fn picus_explain(
    state: &PicusState,
    connectionId: String,
    sql: String,
    analyze: Option<bool>,
    buffers: Option<bool>,
) -> Result<QueryPlan, String> {
    let request =
        PlanRequest { analyze: analyze.unwrap_or(false), buffers: buffers.unwrap_or(false) };
    require_session(state, &connectionId)?
        .explain(&sql, request)
        .await
        .map_err(|e| e.to_string())
}
