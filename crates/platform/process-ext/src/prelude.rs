//! Canonical entry point for `arbor-process-ext`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_process_ext::prelude::...`. The trait also stays re-exported at the
//! crate root for existing call sites.

pub use crate::NoWindowExt;
