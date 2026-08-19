//! Canonical entry point for `arbor-be`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_be::prelude::...`.

pub use crate::app::App;
pub use crate::app_ctx::BackendAppCtx;
pub use crate::dispatch::{Dispatcher, TOOLS_METHOD};
pub use crate::io::BackendIo;
