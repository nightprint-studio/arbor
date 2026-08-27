//! `declaration` domain — `bennu_declaration` (go-to-declaration, Ctrl+Click / Ctrl+B).
//!
//! The navigation twin of `bennu_references`: it classifies the symbol under the caret
//! against the owning project's semantic engine (the whole-project reference index +
//! resolver + source sets built off-thread on `bennu_open_project`) and returns the single
//! DECLARATION site the symbol resolves to — the name token of the method / field / local /
//! class it names, plus the owning project file (with 1-based line/col for the FE to jump
//! to). It reuses the exact caret classification `find_usages` / `rename` share; instead of
//! building edit sites it returns the declaration name span + its file.
//!
//! Returns `None` (never an error) when no project owns the file, the engine is still
//! building, the caret isn't on a resolvable symbol, or the declaration lives in a JDK /
//! dep-jar (no project source to open) — the FE degrades gracefully (no jump).

use bennu_core::prelude::BennuState;
use bennu_proto::prelude::DeclarationTarget;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_declaration`].
#[derive(Deserialize)]
pub struct DeclarationArgs {
    /// Absolute path (forward slashes) to the file the caret is in.
    pub file: String,
    /// The current (possibly-unsaved) buffer text — the caret is classified against this.
    pub source: String,
    /// UTF-8 byte offset of the caret.
    pub offset: usize,
}

/// Resolve the symbol at `file`:`offset` to its declaration site. `None` when no project
/// owns the file, its index is still building, the caret isn't on a resolvable symbol, or
/// the declaration lives in a JDK / dep-jar (no project source to open).
#[arbor_rpc::handler]
fn bennu_declaration(
    _ctx: &BennuState,
    args: DeclarationArgs,
) -> Result<Option<DeclarationTarget>, String> {
    // A language-server-backed file (a `.rs` in a Cargo workspace) is answered by its server.
    // The fall-through matters: routing a Rust buffer into the Java resolver would not fail
    // quietly — that resolver parses anything as Java, so an identifier that happens to exist in
    // the Java index becomes a confident jump into the wrong file.
    if let Some(target) = crate::lsp_route::declaration(&args.file, &args.source, args.offset) {
        return Ok(target);
    }
    // A shader. Before the index for the same reason a `.rs` is: the Java resolver parses
    // anything as Java, so an identifier that happens to exist in the Java index becomes a
    // confident jump into the wrong file.
    if let Some(target) = crate::wgsl_intel::declaration(&args.file, &args.source, args.offset) {
        return Ok(target);
    }
    Ok(IndexService::global().declaration(&args.file, &args.source, args.offset))
}
