//! Canonical entry point for `bennu-proto`'s public API.
//!
//! Workspace convention: call sites (in `bennu-be`, and any future in-process
//! consumer) reach this crate's surface through `bennu_proto::prelude::...`. The
//! `contract` submodule stays `pub` for rustdoc navigation, but the prelude is the
//! canonical call-site path.

pub use crate::contract::{
    BuildDiagnostic, BuildResult, CapabilityHit, CapabilitySet, ClassEntry, CompletionItem,
    Diagnostic, FileContents, FindHit, HoverInfo, IndexStats, JdkInfo, ProjectInfo, RenameEdit,
    RenameFileEdits, RenamePreview, RunHandle, SpellHit, SpellStatus, TodoItem, TreeNode, UsageHit,
    UsagesResult, WriteResult,
};
