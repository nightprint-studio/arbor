//! Canonical entry point for `arbor-ipc`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_ipc::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub use crate::client::{Bytes, BrokerClient, LoopbackBroker};
pub use crate::error::{IpcError, Result};
pub use crate::event::Event;
