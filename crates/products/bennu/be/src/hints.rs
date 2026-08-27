//! `hints` domain — what the editor draws around a call rather than in it.
//!
//! [`bennu_signature_help`] answers for one caret (the strip above the line); [`bennu_inlay_hints`]
//! answers for the whole buffer (the parameter names and inferred `var` types drawn between the
//! code). Both are Java-only and resolver-backed: a language served by a language server gets the
//! same two features from its server, through `lsp_route`.
//!
//! Neither returns an error. A caret that is not in a call, an index still building, a file no
//! project owns — all mean "nothing to draw", and a hint that failed loudly would be a dialog
//! about a tooltip.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_signature_help`].
#[derive(Deserialize)]
pub struct SignatureArgs {
    pub file: String,
    /// The current buffer text — the call is usually half-written, which is the point.
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// The signature strip, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct SignatureWire {
    /// The rendered signature — `transfer(String source, String target, long amount)`.
    pub label: String,
    /// `[start, end)` byte ranges within `label`, one per parameter. Ranges rather than a parameter
    /// list, so the editor marks a span of the very text it is showing.
    pub params: Vec<(usize, usize)>,
    /// Index into `params` of the argument the caret is on.
    pub active: usize,
    /// Byte offset of the call's opening paren — what the strip is anchored to.
    pub anchor: usize,
    /// `[index, count]` when the name was overloaded, absent when it was not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overload: Option<(usize, usize)>,
}

/// The signature of the call the caret is inside, or nothing.
#[arbor_rpc::handler]
fn bennu_signature_help(
    _ctx: &BennuState,
    args: SignatureArgs,
) -> Result<Option<SignatureWire>, String> {
    // A server-backed file answers from its own server; this is the Java engine's reply.
    if !crate::intel::is_java_file(&args.file) {
        return Ok(None);
    }
    Ok(IndexService::global()
        .signature_at(&args.file, &args.source, args.offset)
        .map(|s| SignatureWire {
            label: s.label,
            params: s.params,
            active: s.active,
            anchor: s.anchor,
            overload: s.overload,
        }))
}

/// Args for [`bennu_inlay_hints`].
#[derive(Deserialize)]
pub struct InlayArgs {
    pub file: String,
    pub source: String,
}

/// One hint, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct InlayWire {
    /// **UTF-8 byte offset** the hint is drawn at — the editor maps it, as it does a diagnostic's.
    pub offset: usize,
    pub label: String,
    /// `true` when the hint belongs in front of what is at `offset` rather than behind it.
    pub before: bool,
}

/// Every inlay hint for the buffer (empty when there are none, or the file is not Java).
#[arbor_rpc::handler]
fn bennu_inlay_hints(_ctx: &BennuState, args: InlayArgs) -> Result<Vec<InlayWire>, String> {
    if !crate::intel::is_java_file(&args.file) {
        return Ok(Vec::new());
    }
    Ok(IndexService::global()
        .inlay_hints(&args.file, &args.source)
        .into_iter()
        .map(|h| InlayWire { offset: h.offset, label: h.label, before: h.before })
        .collect())
}
