//! Canonical entry point for `bennu-lsp`'s public API.
//!
//! Workspace convention: every Arbor library crate exposes its public surface through a
//! `prelude` module. Consumers reach types and functions through `bennu_lsp::prelude::...`
//! (or a single `use bennu_lsp::prelude::*;`) rather than through the per-feature submodule
//! paths. The submodules stay `pub` for rustdoc navigation, but call sites go through here.
//!
//! Note what is **not** re-exported: [`crate::types`], the raw protocol structs. A consumer
//! that reaches for `types::Position` is about to do a coordinate conversion that
//! [`LspSession`] already did — the whole point of the [`crate::model`] types is that
//! nothing outside this crate has to think in `{line, character}`.

pub use crate::catalogue::{
    background_init_options, extension_of, find_root, find_root_with_dep, is_dependency_source,
    spec_by_id,
    ServerSpec, BUILTIN_SERVERS,
};
pub use crate::client::{LspError, ServerHandler};
pub use crate::discovery::{locate, locate_custom};
pub use crate::line_index::{LineIndex, Position, PositionEncoding, Range};
pub use crate::snippet::{parse as parse_snippet, Snippet, Stop as SnippetStop};

pub use crate::model::{
    ActionEntry, CompletionEntry, DiagEntry, FileEdit, FileOp, FoldSpan, HierarchyNode,
    HighlightSpan, HoverText, LensEntry, RenameOutcome, ServerAvailability, ServerStatus,
    SessionState, SignatureText, SpanTarget, SymbolNode, TokenSpan,
};
pub use crate::semantic::EMITTED_CLASSES;
pub use crate::session::{LspSession, SessionConfig, SessionObserver, StartFailure};
pub use crate::uri::{from_uri, is_file_uri, to_uri};
