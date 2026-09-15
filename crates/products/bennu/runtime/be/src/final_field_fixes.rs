//! The Alt+Enter repairs for a blank `final` field: *Add constructor parameter*, *Initialize in
//! constructor*, *Initialize variable*, *Make not final* — and, on a Lombok project,
//! *Add `@RequiredArgsConstructor`*.
//!
//! The edits are `bennu-refactor`'s (`final_field_fixes`) and the verdict on which fields are blank is
//! `bennu-check`'s (`uninitialized_final_fields`, the same analysis that draws the squiggle). What is
//! decided here is what only the backend knows: whether Lombok is on this project's classpath, and
//! whether the project already writes `@RequiredArgsConstructor` — in which case that is the fix it
//! wants, and it goes first.
//!
//! ## Two anchors, one set of offers
//!
//! Offered from the **caret** — anywhere on the declaration — and not only from the squiggle, which
//! sits on the name alone and does not exist until validation has run on a field typed a moment ago.
//! A `definite-assignment` diagnostic under the caret is asked as well, but both resolve to the same
//! field, and the first anchor that finds one answers: the menu never lists a repair twice.

use bennu_refactor::prelude::final_field_fixes;

use crate::index_service::IndexService;
use crate::intentions::{
    edit_wire, import_edit_for, offer_of, under_caret, EditWire, IntentionsArgs, OfferCategory,
    RankedOffer,
};

/// The code the blank-final check reports with.
const DIAGNOSTIC_CODE: &str = "definite-assignment";

const REQUIRED_ARGS_CONSTRUCTOR: &str = "lombok.RequiredArgsConstructor";

/// The repairs for the blank final at the caret, as ranked offers — empty anywhere else.
pub(crate) fn final_field_offers(args: &IntentionsArgs) -> Vec<RankedOffer> {
    let source = args.source.as_str();
    // Most Alt+Enter presses are nowhere near a `final`: not worth a parse and a tree walk.
    if !source.contains("final") {
        return Vec::new();
    }
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let root = tree.root_node();
    let blanks = bennu_check::prelude::uninitialized_final_fields(root, source);
    if blanks.is_empty() {
        return Vec::new();
    }
    let anchors = std::iter::once(args.offset)
        .chain(under_caret(args).filter(|d| d.code == DIAGNOSTIC_CODE).map(|d| d.start));
    let Some(found) = anchors.into_iter().find_map(|at| final_field_fixes(root, source, at, &blanks))
    else {
        return Vec::new();
    };

    let mut offers: Vec<RankedOffer> = found
        .fixes
        .into_iter()
        .map(|fix| RankedOffer {
            // The wire's `edits` keep the fix's order, and a single edit travelling in the
            // single-range fields is edit `0` — so the fix's selection indexes the offer unchanged.
            offer: offer_of(&fix.id, &fix.label, fix.edits.iter().map(edit_wire).collect(), fix.select),
            category: OfferCategory::Fix,
            preferred: false,
        })
        .collect();

    if let Some(annotation) =
        found.required_args_constructor.filter(|_| lombok_on_classpath(&args.file, source))
    {
        // The import first: on a file with no package and no imports both edits land at offset 0,
        // and applied in one transaction the import has to come out above the annotation.
        let mut edits: Vec<EditWire> = import_edit_for(source, REQUIRED_ARGS_CONSTRUCTOR)
            .map(|(start, end, text)| EditWire { start, end, text })
            .into_iter()
            .collect();
        edits.push(edit_wire(&annotation));
        let preferred = project_uses_required_args_constructor(&args.file, source);
        let offer = RankedOffer {
            offer: offer_of("add-required-args-constructor", "Add @RequiredArgsConstructor", edits, None),
            category: OfferCategory::Fix,
            preferred,
        };
        // A project that writes its constructors this way wants this one; one that does not is
        // offered it after the plain Java repairs.
        if preferred {
            offers.insert(0, offer);
        } else {
            offers.push(offer);
        }
    }
    offers
}

/// Whether Lombok can compile in this file: it already imports Lombok, or the project's resolved
/// classpath carries `org.projectlombok:lombok` — the coordinate the code templates read "has Lombok"
/// from, so the two never disagree.
fn lombok_on_classpath(file: &str, source: &str) -> bool {
    let imports_lombok = bennu_java::prelude::extract_symbols(source)
        .imports
        .iter()
        .any(|i| i.path == "lombok" || i.path.starts_with("lombok."));
    if imports_lombok {
        return true;
    }
    let service = IndexService::global();
    let Some(root) = service.root_for_file(file) else { return false };
    // `root_for_file` answers forward-slashed; the classpath is looked up by the slot's own key.
    let key = service
        .open_roots()
        .into_iter()
        .find(|r| r.replace('\\', "/").trim_end_matches('/') == root.trim_end_matches('/'))
        .unwrap_or(root);
    crate::templates_facts::facts_for(&key).project.version_of("org.projectlombok:lombok").is_some()
}

/// Whether this project already writes `@RequiredArgsConstructor` — in this file, or imported by any
/// other file the project's import census counted.
fn project_uses_required_args_constructor(file: &str, source: &str) -> bool {
    source.contains("@RequiredArgsConstructor")
        || IndexService::global().import_count(file, REQUIRED_ARGS_CONSTRUCTOR) > 0
}
