//! Canonical entry point for `corvus-brp`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_brp::prelude::...` (or a single `use corvus_brp::prelude::*;`). The
//! submodules stay `pub` for rustdoc navigation but are not the canonical
//! call-site path.

pub use crate::client::{BrpClient, BrpError};
pub use crate::sse::{run_watch_stream, WatchEvent};
pub use crate::{
    methods, probe_capabilities, BrpCapabilities, BrpRegistry, BrpSession, BrpStatus, WatchSub,
    DEFAULT_ENDPOINT,
};
