//! Canonical entry point for `bennu-mybatis`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_mybatis::prelude::...`.

// The extension itself — what a host registers.
pub use crate::ext::MyBatisExtension;
