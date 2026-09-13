//! `corvus-be`'s answer to `__memory` — what this backend is holding, and for which repository.
//!
//! Corvus keeps little: a git2 `Repository` is opened per call and dropped with it, and job output
//! lives in the shell. What stays is memoised — per-tab statistics, the ticket links of the commits
//! a graph has shown, the avatars of the authors it has drawn. The plugin VMs are listed by the
//! runtime (`arbor-be`), not here.

use std::collections::HashMap;

use arbor_be::prelude::{json_heap_estimate, MemoryItem, MemoryReport, PROCESS_SCOPE};
use corvus_core::prelude::CorvusState;

/// Everything this backend can say about its own memory.
pub(crate) fn report(state: &CorvusState) -> MemoryReport {
    let tabs: HashMap<String, String> = state.open_tabs().into_iter().collect();
    // A cache entry whose tab has closed is still held — it goes under the whole backend, which is
    // exactly where it should show: memory no open repository accounts for.
    let scope_of = |tab: &str| tabs.get(tab).cloned().unwrap_or_else(|| PROCESS_SCOPE.to_string());

    let mut items = vec![MemoryItem::counted(PROCESS_SCOPE, "Repositories open", tabs.len())];

    if let Ok(cache) = state.stats_cache().lock() {
        for (tab, (_, stats)) in cache.iter() {
            let label = if tabs.contains_key(tab) {
                "Repository statistics"
            } else {
                "Statistics of a closed tab"
            };
            items.push(MemoryItem::estimate(scope_of(tab), label, 1, json_heap_estimate(stats)));
        }
    }

    for (tab, entries) in crate::tickets::memory_by_tab() {
        items.push(MemoryItem::counted(scope_of(&tab), "Ticket links looked up, per commit", entries));
    }

    items.push(MemoryItem::counted(
        PROCESS_SCOPE,
        "Commit avatars looked up",
        corvus_git_provider_api::prelude::avatar_cache_len(),
    ));

    MemoryReport { items }
}
