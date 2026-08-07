//! Canonical entry point for `bennu-proto`'s public API.
//!
//! Workspace convention: call sites (in `bennu-be`, and any future in-process
//! consumer) reach this crate's surface through `bennu_proto::prelude::...`. The
//! `contract` and `lsp` submodules stay `pub` for rustdoc navigation, but the prelude is
//! the canonical call-site path.

pub use crate::contract::{
    Breakpoint, BreakpointStatus, BuildDiagnostic, BuildResult, CapabilityHit, CapabilitySet,
    ClassEntry, CompletionItem, DebugConfig, DebugPause, DebugStatus, DebugValue,
    DeclarationTarget, Diagnostic, EncodingIssue, EnvVar, ExceptionBreakpoint, FileContents,
    FileDiagnostics, FileStamp, FileValidationStat, FindHit, FormAnalysis, StackFrame,
    ERR_EXTERNALLY_MODIFIED,
    FormFieldInfo, FormInfo, HoverInfo, IndexEntry, IndexStats,
    JdkStatus, JspActionBinding, JspActionOption, JspNav, PropertyLintHit,
    InheritedMember, InheritedSource, JdkInfo, MainClassEntry, ProjectInfo, ProjectKind,
    ProjectValidationResult,
    RenameEdit, RenameFileEdits, RenamePreview, RunConfig, RunConfigSet, RunHandle, SnippetStop, SpellHit,
    SpellStatus,
    TodoItem, TreeNode, UsageHit, UsagesResult, ValidationContext, WriteResult,
};
pub use crate::lsp::{
    LspAction, LspCallSite, LspDiagnostic, LspFold, LspHierarchyNode, LspHighlight, LspLens,
    LspMacroExpansion, LspRelated, LspServerInfo, LspSignature, LspStatus, LspSymbol, LspToken,
    SourceEdit,
};
