//! `hover` domain — `bennu_hover` (editor hover card).
//!
//! The read-only cousin of `bennu_references`: it classifies the symbol under the caret
//! against the owning project's rename engine (the whole-project reference index +
//! resolver built off-thread on `bennu_open_project`) and returns a hover card — the
//! symbol's signature, its kind, and (for a member) its owning type's dotted FQCN.
//!
//! Returns `None` (never an error) when no project owns the file, the engine is still
//! building, or the caret isn't on a symbol we can classify (a local variable / parameter
//! isn't keyed here) — the FE degrades gracefully (no hover card).
//!
//! Javadoc extraction is deferred: `doc` is always `None` for now (see the design note).

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::HoverInfo;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_hover`].
#[derive(Deserialize)]
pub struct HoverArgs {
    /// Absolute path (forward slashes) to the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against this.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// Resolve the hover card for the symbol at `file`:`offset`. `None` when no project owns
/// the file, its index is still building, or the caret isn't on a classifiable symbol.
#[arbor_rpc::handler]
fn bennu_hover(_ctx: &BennuState, args: HoverArgs) -> Result<Option<HoverInfo>, String> {
    // A server-backed file answers from its own server, whose hover is markdown — split into
    // the card's signature / container / doc slots by `lsp_route`.
    if let Some(card) = crate::lsp_route::hover(&args.file, &args.source, args.offset) {
        return Ok(card);
    }
    // A shader: its own declarations with the comment block above them as documentation, and
    // the language's built-ins with what the language says they are.
    if let Some(card) = crate::wgsl_intel::hover(&args.file, &args.source, args.offset) {
        return Ok(card);
    }
    Ok(IndexService::global().hover(&args.file, &args.source, args.offset))
}
