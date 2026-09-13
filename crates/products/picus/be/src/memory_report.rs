//! `picus-be`'s answer to `__memory` — the script repositories read, and what each database said
//! about itself.
//!
//! Both caches hold until told otherwise (a refresh, a write, a re-read), so their lines are what
//! grows over a session. The cursors and relation names inside each live session sit behind the
//! engine's session trait and are not listed; the session count stands for them. The plugin VMs,
//! when there is a host, are listed by the runtime (`arbor-be`), not here.

use std::mem::size_of_val;

use arbor_be::prelude::{MemoryItem, MemoryReport, PROCESS_SCOPE};
use picus_core::prelude::PicusState;

/// Everything this backend can say about its own memory.
pub(crate) fn report(state: &PicusState) -> MemoryReport {
    let mut items = Vec::new();

    for snapshot in state.scripts().snapshots() {
        let bytes = snapshot.sources.values().map(|s| s.text.capacity()).sum();
        items.push(MemoryItem::exact(
            snapshot.root.to_string_lossy(),
            "Scripts held as text",
            snapshot.sources.len(),
            bytes,
        ));
    }

    for (id, schema, definitions) in state.schemas().held() {
        // Named as the user named it; the id is what is left when the connection was deleted
        // while its schema was still held.
        let name = crate::connections::find_spec(&id).map(|s| s.name).unwrap_or(id);
        let relations = schema.tables.len() + schema.views.len();
        let bytes: usize = schema
            .tables
            .iter()
            .chain(&schema.views)
            .map(|t| {
                t.name.capacity()
                    + size_of_val(t.columns.as_slice())
                    + t.columns
                        .iter()
                        .map(|c| c.name.capacity() + c.data_type.capacity())
                        .sum::<usize>()
                    + t.definition.as_ref().map_or(0, String::capacity)
            })
            .sum::<usize>()
            + definitions.as_deref().map_or(0, |defs| {
                defs.iter().map(|(n, sql)| n.capacity() + sql.capacity()).sum()
            });
        items.push(MemoryItem::estimate(PROCESS_SCOPE, format!("Schema of {name}"), relations, bytes));
    }

    items.push(MemoryItem::counted(PROCESS_SCOPE, "Database sessions open", state.sessions().open_ids().len()));

    MemoryReport { items }
}
