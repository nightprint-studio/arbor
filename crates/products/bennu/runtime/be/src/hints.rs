//! `hints` domain — what the editor draws around a call rather than in it.
//!
//! [`bennu_signature_help`] answers for one caret (the strip above the line); [`bennu_inlay_hints`]
//! answers for the whole buffer (the parameter names and inferred `var` types drawn between the
//! code); [`bennu_usage_counts`] answers for the whole buffer too — how many places use each of its
//! declarations, and which of them nothing reaches. All are Java-only and resolver-backed: a
//! language served by a language server gets the same features from its server, through
//! `lsp_route`.
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
    /// What the hint says on hover — the parameter's declared type. Omitted when there is nothing
    /// more to say than the label already does.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tooltip: String,
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
        .map(|h| InlayWire {
            offset: h.offset,
            label: h.label,
            before: h.before,
            tooltip: h.tooltip,
        })
        .collect())
}

/// Args for [`bennu_usage_counts`].
#[derive(Deserialize)]
pub struct UsageCountArgs {
    pub file: String,
    pub source: String,
    /// For a **library source view** — a file under no project: the project file it was opened from,
    /// whose index holds the uses. Absent for an ordinary file.
    #[serde(default)]
    pub origin_file: Option<String>,
}

/// One declaration's use count, on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct UsageCountWire {
    /// Byte offset of the declaration, annotations included — where the row above it is drawn.
    pub decl: usize,
    /// Byte span of the NAME token — what a "nothing reaches this" tint colours.
    pub start: usize,
    pub end: usize,
    /// `"type"` | `"method"` | `"field"`.
    pub kind: String,
    /// The declared name.
    pub name: String,
    /// How many use sites the index holds. The declaration is not one of them.
    pub count: usize,
    /// True only when the count of zero is a fact about the **program** and not merely about the
    /// index — see `bennu_intel::usage_marks`. The editor greys exactly these.
    pub unused: bool,
    /// Text drawn after the count on the same row — for a Patina function, its static gas estimate (`~gas 24 + 8/iter · cx 3`). Absent for Java.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The note is a warning (the function costs more than the mod's `per_hook` budget): draw the row in the accent tone.
    #[serde(skip_serializing_if = "is_false")]
    pub warn: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// How many places use each declaration in the buffer.
///
/// **Not** one per declaration: an entry point nothing else calls is left out entirely, so the
/// caller draws what it is given and never has to decide what is worth saying.
///
/// Java-only, and empty — never an error — while the index is still building or for a file no
/// project owns. A count that is one edit stale is worth having; a dialog about one is not.
#[arbor_rpc::handler]
fn bennu_usage_counts(
    _ctx: &BennuState,
    args: UsageCountArgs,
) -> Result<Vec<UsageCountWire>, String> {
    // A `.ptn` file is answered by the Patina package: the same rows, plus the gas estimate of each function.
    if let Some(marks) = crate::patina_intel::usage_marks(_ctx, &args.file, &args.source) {
        return Ok(marks);
    }
    if !crate::intel::is_java_file(&args.file) {
        return Ok(Vec::new());
    }
    let service = IndexService::global();
    // A library view belongs to no project, so its own path cannot pick the index — the origin does.
    // What the index knows about a library is the uses of its TYPES (the project's own references to
    // them); a count of zero, or anything about its members, is not a fact about the program, so
    // only types with at least one use are drawn there and nothing is ever greyed out.
    let library = args.origin_file.is_some() && service.root_for_file(&args.file).is_none();
    let marks = match (&args.origin_file, library) {
        (Some(origin), true) => service.usage_marks_from(origin, &args.file, &args.source),
        _ => service.usage_marks(&args.file, &args.source),
    };
    Ok(marks
        .into_iter()
        .filter(|m| !library || (m.kind.to_string() == "type" && m.count > 0))
        // A declaration a framework calls, that nothing else does, is dropped rather than sent
        // with an explanation attached. "no usages · the test runner runs it" above every method
        // of a test file is true, useless, and the reason the editor was in the way.
        .filter(|m| !m.is_silent())
        .map(|m| {
            let unused = !library && m.is_unused();
            UsageCountWire {
                decl: m.decl,
                start: m.start,
                end: m.end,
                kind: m.kind.to_string(),
                name: m.name,
                count: m.count,
                unused,
                note: None,
                warn: false,
            }
        })
        .collect())
}
