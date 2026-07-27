//! `schema` domain — reading what a live connection contains.
//!
//! Two granularities on purpose. [`picus_read_schema`] is the tree: every relation
//! with its columns, cheap enough for a schema with hundreds of tables.
//! [`picus_table_detail`] adds the constraints and indexes, and is only paid for
//! when a tab actually opens.

use picus_core::prelude::PicusState;
use picus_db_api::prelude::{RowPage, SchemaSnapshot, TableInfo};

use crate::connections::require_session;

/// The whole schema of an open connection: tables, views, sequences, triggers.
#[arbor_rpc::handler]
async fn picus_read_schema(state: &PicusState, id: String) -> Result<SchemaSnapshot, String> {
    require_session(state, &id)?.read_schema().await.map_err(|e| e.to_string())
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

/// One page of a relation's rows.
///
/// Paging bounds what crosses the wire; the grid's virtualisation bounds what is
/// drawn. Both are needed, which is why the page size goes as high as it does.
#[arbor_rpc::handler]
async fn picus_fetch_page(
    state: &PicusState,
    id: String,
    name: String,
    offset: u64,
    limit: u32,
) -> Result<RowPage, String> {
    require_session(state, &id)?
        .fetch_page(&name, offset, limit)
        .await
        .map_err(|e| e.to_string())
}
