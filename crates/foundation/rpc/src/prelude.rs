//! Canonical entry point for `arbor-rpc`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_rpc::prelude::...`.

pub use crate::{decode_field, handler, registry, registry_for, CallFn, Entry};
