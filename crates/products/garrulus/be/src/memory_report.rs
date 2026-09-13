//! `garrulus-be`'s answer to `__memory` — what the open vault costs to keep searchable.
//!
//! Almost all of it is the index: the text of every note (search and snippets are cut from it), the
//! word index over that text, the link graph and the unlinked mentions. The index sizes itself
//! (`Index::footprint`); this only turns the numbers into lines. The plugin VMs are listed by the
//! runtime (`arbor-be`), not here.

use arbor_be::prelude::{MemoryItem, MemoryReport, PROCESS_SCOPE};
use garrulus_core::prelude::GarrulusState;

/// Everything this backend can say about its own memory.
pub(crate) fn report(state: &GarrulusState) -> MemoryReport {
    let scope = state
        .vault_read()
        .ok()
        .and_then(|vault| (*vault).as_ref().map(|v| v.root.to_string_lossy().to_string()))
        .unwrap_or_else(|| PROCESS_SCOPE.to_string());

    let mut items = Vec::new();
    if let Ok(index) = state.index_read() {
        let f = index.footprint();
        items.push(MemoryItem::exact(&scope, "Note text held for search", f.bodies, f.body_bytes));
        items.push(MemoryItem::estimate(&scope, "Word index", f.words, f.word_index_bytes));
        items.push(MemoryItem::estimate(&scope, "Titles, tags, links and properties", f.notes, f.metadata_bytes));
        items.push(MemoryItem::estimate(&scope, "Unlinked mentions", f.mentions, f.mention_bytes));
        items.push(MemoryItem::counted(&scope, "Links and backlinks", f.links));
    }
    items.push(MemoryItem::counted(PROCESS_SCOPE, "Conflicts from the last pull", crate::sync::conflict_count()));

    MemoryReport { items }
}
