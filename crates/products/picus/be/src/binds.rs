//! `binds` domain — running a statement whose values are sent beside it.
//!
//! The sibling of [`picus_execute`](crate::query::picus_execute), and deliberately
//! a **second door** rather than an optional argument on the first one. Two reasons,
//! and both are about what a caller can be sure of:
//!
//! * binding is a **capability**. `EngineCapabilities::bind_parameters` says whether
//!   an engine has it, and the interface reads the flag instead of calling and
//!   catching — a feature the engine lacks has to be absent, not present and
//!   failing. A `binds` key quietly ignored by an engine that cannot bind would be
//!   exactly the second kind;
//! * the **result differs**, and it differs in a way the caller must see. A cursor
//!   is a utility statement and takes no parameters, so a bound read opens no
//!   scrollable result: `resultId` is null and `endOfResult` is the honest statement
//!   of whether anything was left behind. Folding that into `picus_execute` would
//!   make the same call sometimes scrollable and sometimes not.
//!
//! The wire keys are `connectionId`, so the argument is spelled that way — the
//! contract wins over the naming convention, as it does in [`crate::query`].
#![allow(non_snake_case)]

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{BindValue, ExecuteResult};

use crate::connections::require_session;
use crate::query::window_size;

/// Run a statement with its values bound to its placeholders.
///
/// `binds` is **positional**: `binds[0]` is `$1` on PostgreSQL and the first
/// placeholder in order of appearance on Oracle. A `null` entry is a real SQL NULL
/// and not the empty string — the interface asks for the two separately, because in
/// a maintenance tool confusing them is how a bad `UPDATE` gets written.
///
/// Nothing here inspects the SQL: what a statement is, how many values it wants and
/// whether it produces rows are all questions the engine asks the server. A caller
/// that had to classify SQL first would get it wrong silently.
#[arbor_rpc::handler]
async fn picus_execute_bound(
    state: &PicusState,
    connectionId: String,
    sql: String,
    binds: Vec<BindValue>,
    window: Option<u32>,
) -> Result<ExecuteResult, String> {
    require_session(state, &connectionId)?
        .execute_bound(&sql, &binds, window_size(window))
        .await
        .map_err(|e| e.to_string())
}
