//! Canonical entry point for `bennu-proto`'s public API.
//!
//! Workspace convention: call sites (in `bennu-be`, and any future in-process
//! consumer) reach this crate's surface through `bennu_proto::prelude::...`. The
//! `contract` submodule stays `pub` for rustdoc navigation, but the prelude is the
//! canonical call-site path.

pub use crate::contract::{
    CapabilityHit, CapabilitySet, CompletionItem, Diagnostic, FileContents, JdkInfo, ProjectInfo,
    TreeNode,
};
