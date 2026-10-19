//! "Import static" — the Alt+Enter offer for a static member written without its class.
//!
//! `MAX_RETRIES` or `assertThat(x)` with nothing in front of it is an unresolved symbol / call until
//! `import static pkg.Owner.NAME;` is added. The offer is gated on the diagnostic the validator
//! already raised rather than on the text under the caret: the tree cannot tell a bare name that
//! needs an import from a local, an inherited member or an outer class's field, and offering an
//! import for a name that already resolves would be noise. The validator knows, and says so.
//!
//! The owners come from the provider's static-member index, which is filled in the background — see
//! [`bennu_intel::prelude::StaticMemberIndex`]. The edit is the pure `insert_static_import_edit`,
//! shared with the framework quick-fixes that add a static import.

use bennu_intel::prelude::StaticKind;

use crate::index_service::IndexService;
use crate::intentions::{under_caret, IntentionsArgs, OfferCategory, OfferWire, RankedOffer};

/// Caps a pathologically common name (`of`, `get`) so it cannot flood the menu.
const MAX_CANDIDATES: usize = 25;

/// The "Import static '<owner>.<name>'" offers for the unresolved bare names under the caret.
pub(crate) fn static_import_offers(args: &IntentionsArgs) -> Vec<RankedOffer> {
    let mut out: Vec<RankedOffer> = Vec::new();
    for diagnostic in under_caret(args) {
        // `LIMIT` is an unresolved *symbol*, `limit()` an unresolved *call*; the code says which
        // table to ask, and anything else is another repair's business.
        let kind = match diagnostic.code.as_str() {
            "unresolved-symbol" => StaticKind::Field,
            "unresolved-call" => StaticKind::Method,
            _ => continue,
        };
        let Some(name) = bare_name(&args.source, diagnostic.start, diagnostic.end, kind) else { continue };
        for offer in offers_for(&args.file, &args.source, &name, kind) {
            if !out.iter().any(|o| o.offer.id == offer.offer.id) {
                out.push(offer);
            }
        }
    }
    out
}

fn offers_for(file: &str, source: &str, name: &str, kind: StaticKind) -> Vec<RankedOffer> {
    let choices = IndexService::global().static_import_choices(file, source, name, kind);
    let mut out = Vec::new();
    for (place, owner) in choices.fqns.iter().enumerate() {
        // None when the member is already imported, by name or through `import static Owner.*;`.
        let Some(edit) = bennu_intentions::prelude::insert_static_import_edit(source, owner, name) else {
            continue;
        };
        out.push(RankedOffer {
            offer: OfferWire {
                id: format!("import-static:{owner}.{name}"),
                label: format!("Import static '{owner}.{name}'"),
                start: edit.start,
                end: edit.end,
                replacement: edit.replacement,
                action: None,
                edits: Vec::new(),
                select: None,
            },
            category: OfferCategory::Fix,
            preferred: place == 0 && choices.clear_winner,
        });
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    out
}

/// The bare member name a diagnostic spans, or `None` when it is not one: a qualified use
/// (`Limits.MAX` — the class is already written), or a span that is more than a name.
///
/// A call's span is the callee name in what the validator reports today, but a whole `name(args)` is
/// read the same way, so the offer does not hang on that detail.
fn bare_name(source: &str, start: usize, end: usize, kind: StaticKind) -> Option<String> {
    let text = source.get(start..end)?.trim();
    let name: String = text.chars().take_while(|c| is_identifier_char(*c)).collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let rest = text[name.len()..].trim_start();
    let shaped = match kind {
        StaticKind::Field => rest.is_empty(),
        StaticKind::Method => rest.is_empty() || rest.starts_with('('),
    };
    // Whatever stands before the name decides whether it was written bare.
    let qualified = source[..start].trim_end().ends_with('.');
    (shaped && !qualified).then_some(name)
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name_in(source: &str, needle: &str, len: usize, kind: StaticKind) -> Option<String> {
        let at = source.find(needle).expect("needle present");
        bare_name(source, at, at + len, kind)
    }

    #[test]
    fn a_bare_constant_is_a_field_name() {
        let src = "class C { int m() { return MAX_RETRIES; } }";
        assert_eq!(name_in(src, "MAX_RETRIES", 11, StaticKind::Field).as_deref(), Some("MAX_RETRIES"));
    }

    #[test]
    fn a_bare_call_is_a_method_name_whether_the_span_is_the_name_or_the_call() {
        let src = "class C { void m() { assertThat(x); } }";
        assert_eq!(name_in(src, "assertThat", 10, StaticKind::Method).as_deref(), Some("assertThat"));
        assert_eq!(name_in(src, "assertThat", 14, StaticKind::Method).as_deref(), Some("assertThat"));
    }

    #[test]
    fn a_qualified_use_is_not_bare() {
        let src = "class C { int m() { return Limits.MAX; } }";
        assert_eq!(name_in(src, "MAX", 3, StaticKind::Field), None);
        let spaced = "class C { void m() { Util\n    .help(); } }";
        assert_eq!(name_in(spaced, "help", 4, StaticKind::Method), None);
    }

    #[test]
    fn a_span_that_is_more_than_a_name_is_declined() {
        // A field span with an argument list is not a field; a call span with a trailing expression is
        // not a callee.
        let src = "class C { void m() { go(1) + 2; } }";
        assert_eq!(name_in(src, "go(1)", 5, StaticKind::Field), None);
        assert_eq!(name_in(src, "go", 0, StaticKind::Method), None);
    }

    #[test]
    fn a_span_outside_the_source_is_declined() {
        assert_eq!(bare_name("class C {}", 5, 500, StaticKind::Field), None);
    }
}
