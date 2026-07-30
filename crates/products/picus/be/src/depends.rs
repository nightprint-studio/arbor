//! `depends` domain — the dependency graph of the connected schema.
//!
//! One handler, and deliberately no caching here. The graph is a fixed handful of catalogue
//! queries; it is asked for when a panel opens and not on a keystroke, and the
//! frontend holds the answer for as long as the connection lives (invalidated by
//! hand, like the schema snapshot beside it). A second cache in the backend would
//! only add a second thing to forget to invalidate.
//!
//! The engine may not have the concept at all — the trait's default refuses with
//! `DbError::Unsupported`, and that string crosses the seam like any other. The
//! interface is not supposed to get there: `EngineCapabilities::dependency_graph`
//! says whether the panel exists, so an engine without one has no button rather
//! than a button that fails.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::DependencyGraph;

use crate::connections::require_session;

/// Every object of this connection's schema and every relationship the catalogue
/// records between them.
///
/// Its parameter is `id`, like the rest of the schema-reading family
/// (`picus_read_schema`, `picus_table_detail`, `picus_trigger_detail`) rather than
/// the `connectionId` the query family uses. This is a read *about the catalogue*,
/// it is asked the same way and it lives in the same panel neighbourhood; spelling
/// its one argument differently from its neighbours would be a difference with no
/// meaning behind it.
#[arbor_rpc::handler]
async fn picus_dependencies(state: &PicusState, id: String) -> Result<DependencyGraph, String> {
    require_session(state, &id)?.dependencies().await.map_err(|e| e.to_string())
}
