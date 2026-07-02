//! `intel` domain — `bennu_completion` / `bennu_diagnostics`.
//!
//! `bennu_completion` serves member-access candidates from the per-project index the
//! [`crate::index_service`] builds off-thread on `bennu_open_project`. Until that build
//! lands (or when no open project owns the file), it returns the benign empty list —
//! the FE shows nothing gracefully. `bennu_diagnostics` stays a stub routed through the
//! native provider so the seam is exercised; syntactic diagnostics land in a later wave.

use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{CompletionItem, IntelProvider, NativeJavaProvider};
use bennu_proto::prelude::Diagnostic;
use serde::Deserialize;

use crate::index_service::IndexService;

/// Args for [`bennu_completion`].
#[derive(Deserialize)]
pub struct CompletionArgs {
    /// Absolute path to the file the caret is in.
    pub file: String,
    /// Byte offset of the caret in the file.
    pub offset: usize,
}

/// Completion candidates at a position — served from the owning project's built index
/// (empty while the index is still building, per the async lifecycle).
#[arbor_rpc::handler]
fn bennu_completion(_ctx: &BennuState, args: CompletionArgs) -> Result<Vec<CompletionItem>, String> {
    Ok(IndexService::global().completion(&args.file, args.offset))
}

/// Args for [`bennu_diagnostics`].
#[derive(Deserialize)]
pub struct DiagnosticsArgs {
    /// Absolute path to the file to diagnose.
    pub file: String,
}

/// Diagnostics for a file (Phase-0 stub → `[]` via the native provider).
#[arbor_rpc::handler]
fn bennu_diagnostics(_ctx: &BennuState, args: DiagnosticsArgs) -> Result<Vec<Diagnostic>, String> {
    let provider = NativeJavaProvider::new();
    provider.diagnostics(&args.file).map_err(|e| e.to_string())
}
