//! Canonical entry point for `arbor-be`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_be::prelude::...`.

pub use crate::app::App;
pub use crate::app_ctx::BackendAppCtx;
pub use crate::dispatch::{
    Dispatcher, MemoryItem, MemoryReport, MEMORY_METHOD, PROCESS_SCOPE, TOOLS_METHOD,
};
pub use crate::memory::json_heap_estimate;
pub use crate::focus::{app_focused, set_app_focused, FOCUS_METHOD};
pub use crate::io::BackendIo;
