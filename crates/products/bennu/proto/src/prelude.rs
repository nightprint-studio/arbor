//! Canonical entry point for `bennu-proto`'s public API.
//!
//! Workspace convention: call sites (in `bennu-be`, and any future in-process
//! consumer) reach this crate's surface through `bennu_proto::prelude::...`. The
//! `contract` submodule stays `pub` for rustdoc navigation, but the prelude is the
//! canonical call-site path.

pub use crate::contract::{
    BuildDiagnostic, BuildResult, CapabilityHit, CapabilitySet, ClassEntry, CompletionItem,
    DeclarationTarget, Diagnostic, EncodingIssue, EnvVar, FileContents, FindHit, FormAnalysis,
    FormFieldInfo, FormInfo, HoverInfo, IndexEntry, IndexStats,
    JdkStatus, JspNav,
    InheritedMember, InheritedSource, JdkInfo, MainClassEntry, ProjectInfo, RenameEdit,
    RenameFileEdits, RenamePreview, RunConfig, RunConfigSet, RunHandle, SpellHit, SpellStatus,
    TodoItem, TreeNode, UsageHit, UsagesResult, ValidationContext, WriteResult,
};
