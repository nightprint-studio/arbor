//! `sitta-be`'s answer to `__memory`.
//!
//! The explorer reads the disk on every call and keeps nothing of its own; what it does keep is the
//! git status behind the overlay badges, one entry per repository browsed, in `corvus_git`'s
//! explorer. The plugin VMs are listed by the runtime (`arbor-be`), not here.

use arbor_be::prelude::{MemoryItem, MemoryReport};

/// Everything this backend can say about its own memory.
pub(crate) fn report() -> MemoryReport {
    let items = corvus_git::explorer::status_cache_footprint()
        .into_iter()
        .map(|(root, files, bytes)| MemoryItem::estimate(root, "Git status behind the badges", files, bytes))
        .collect();
    MemoryReport { items }
}
