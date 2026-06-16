//! Canonical entry point for `arbor-shell-common`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_shell_common::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation.

pub use crate::broker::{BrokerError, CredentialBroker};
pub use crate::router::{Router, RouterError};
