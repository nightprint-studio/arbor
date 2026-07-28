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
use picus_db_api::prelude::{SchemaSnapshot, TableInfo};

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
