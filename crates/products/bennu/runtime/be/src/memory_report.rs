//! `bennu-be`'s answer to `__memory` — what this backend is holding, and for which project.
//!
//! One function, and deliberately thin: each engine sizes its own structures where its fields are
//! (the index service, the library index, the framework host, the language-server sessions, the
//! shader library, the crate catalogue), and this only puts the lines together. A breakdown
//! assembled here from outside would have to reach into private state, and would be the first
//! thing to go stale the day one of those structures changed.
//!
//! Language servers appear only for what *this* process keeps for them — the open files' text, the
//! diagnostics, the last completion list. The servers themselves are processes of their own, and
//! the monitor measures each as a row.

use arbor_be::prelude::MemoryReport;

use crate::frameworks::FrameworkService;
use crate::index_service::IndexService;
use crate::lsp_registry::LspRegistry;

/// Everything this backend can say about its own memory.
pub(crate) fn report() -> MemoryReport {
    let mut items = IndexService::global().memory_report();
    items.extend(crate::library_search::memory_items());
    items.extend(FrameworkService::global().memory_items());
    items.extend(LspRegistry::global().memory_items());
    items.extend(crate::wgsl_library::memory_items());
    items.extend(crate::cargo_intel::memory_items());
    MemoryReport { items }
}
