//! `intentions` domain — `bennu_intentions_at`: every Alt+Enter quick-fix applicable at the caret.
//!
//! One handler over the whole `bennu-intentions` catalog: it returns the applicable transforms as
//! offers (id + label + edit), so the editor makes a single round-trip per Alt+Enter and adding an
//! intention is a change in the pure crate only. Replaces the old per-transform handlers.

use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};

use crate::index_service::IndexService;

/// Args for [`bennu_intentions_at`].
#[derive(Deserialize)]
pub struct IntentionsArgs {
    /// Absolute path of the file — drives the file-context intentions (fix the package to match the
    /// file's location).
    pub file: String,
    /// The current buffer text.
    pub source: String,
    /// Caret position as a **UTF-8 byte offset** into `source`.
    pub offset: usize,
}

/// One applicable intention — a stable id, a label, and either a byte-range edit to apply or an
/// `action` the editor runs instead (e.g. a filesystem move). `action` is `None` for a plain edit.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OfferWire {
    pub id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
    /// A non-edit action id the editor dispatches instead of applying the edit (`"move-to-package"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

/// Every intention applicable at the caret (empty when none fits).
#[arbor_rpc::handler]
fn bennu_intentions_at(_ctx: &BennuState, args: IntentionsArgs) -> Result<Vec<OfferWire>, String> {
    let mut offers: Vec<OfferWire> = Vec::new();

    // Everything below this point up to the naming offer is **Java**, and is now guarded as such:
    // the naming pack covers server-backed languages too, so the editor asks this handler for a
    // `.ts` and a `.rs` as well. Running Java source transforms over TypeScript would offer edits
    // computed from a grammar that never read it.
    if crate::intel::is_java_file(&args.file) {
        // The pure source transforms (caret-anchored).
        offers.extend(
            bennu_intentions::prelude::intentions_at(&args.source, args.offset).into_iter().map(
                |o| OfferWire {
                    id: o.id,
                    label: o.label,
                    start: o.start,
                    end: o.end,
                    replacement: o.replacement,
                    action: None,
                },
            ),
        );
        offers.extend(java_file_offers(&args.file, &args.source, args.offset));
    }

    // The caret is on a declaration whose name breaks the project's naming convention. The fix is
    // the name the convention itself produced, so the offer never has to compute one.
    //
    // Dispatched as a **rename action**, not as an edit — even for a local, where replacing the
    // identifier in place would rewrite the declaration and leave every use of it behind. The
    // project's rename engine is what knows the others.
    //
    // Which of the two rename actions depends on how far the rename can reach, which the pack
    // decides: only a declaration a *grammar* found, whose kind cannot be referred to from outside
    // its file, is applied on the spot — asking a user to confirm a preview that can only ever list
    // one file is a dialog that teaches them to click through dialogs. Everything else — every
    // method, field and type, and *everything* a language server's outline reported — opens the
    // preview with the suggestion filled in, and the user decides.
    if let Some(violation) = crate::naming::violation_at(&args.file, &args.source, args.offset) {
        let action = if violation.file_local { "rename-symbol" } else { "rename-symbol-preview" };
        offers.push(OfferWire {
            id: format!("naming-fix:{}", violation.target),
            label: format!("Rename to `{}`", violation.suggested),
            start: violation.start,
            end: violation.end,
            // The action's payload: what to rename to. The editor plans the rename at `start`.
            replacement: violation.suggested,
            action: Some(action.to_string()),
        });
    }

    Ok(offers)
}

/// The Java intentions that need the file's location rather than only its text: fixing a package
/// mismatch, and importing a type the caret is on.
fn java_file_offers(file: &str, source: &str, offset: usize) -> Vec<OfferWire> {
    let mut offers: Vec<OfferWire> = Vec::new();

    // File-context intentions on a package mismatch: (a) rewrite the declaration to match the folder,
    // or (b) move the file to the folder matching the declaration. Both anchored on the same mismatch.
    if let Some(expected) = std::path::Path::new(file)
        .parent()
        .and_then(bennu_java::prelude::infer_package)
    {
        if let Some((start, end, replacement)) =
            bennu_check::prelude::change_package(source, &expected)
        {
            offers.push(OfferWire {
                id: "change-package".to_string(),
                label: format!("Set package to `{expected}`"),
                start,
                end,
                replacement,
                action: None,
            });
            // The move alternative — a filesystem action, not an edit (dispatched by the editor).
            if let Some(declared) = bennu_java::prelude::extract_symbols(source).package {
                offers.push(OfferWire {
                    id: "move-to-package".to_string(),
                    label: format!("Move file to package `{declared}`"),
                    start: 0,
                    end: 0,
                    replacement: String::new(),
                    action: Some("move-to-package".to_string()),
                });
            }
        }
    }

    // "Import class": the caret is on a bare, unimported type name → offer to add its import, one
    // offer per candidate FQN (the Alt+Enter menu is the "which import?" picker the user asked for).
    if let Some(simple) = bennu_java::prelude::simple_type_needing_import(source, offset) {
        offers.extend(import_class_offers(file, source, &simple));
    }
    offers
}

/// Build the "Import `<fqn>`" offers for the unimported simple type `simple` used in `source`: one per
/// candidate FQN from the project's class-name index, minus those that need NO import (see
/// [`import_edit_for`]). Each carries the edit that inserts the `import …;` line. Capped so a
/// pathologically common name can't flood the menu.
fn import_class_offers(file: &str, source: &str, simple: &str) -> Vec<OfferWire> {
    const MAX_CANDIDATES: usize = 25;

    let mut out = Vec::new();
    for fqn in IndexService::global().import_candidates(file, simple) {
        let Some((start, end, replacement)) = import_edit_for(source, &fqn) else {
            continue; // needs no import (java.lang / same package / star / already imported)
        };
        out.push(OfferWire {
            id: format!("import-class:{fqn}"),
            label: format!("Import '{fqn}'"),
            start,
            end,
            replacement,
            action: None,
        });
        if out.len() >= MAX_CANDIDATES {
            break;
        }
    }
    out
}

/// The edit that adds `import <fqn>;` to `source` (byte range + replacement), or `None` when no import
/// is needed: the fqn is a `java.lang` type, in the file's OWN package, already imported, or covered
/// by an `import pkg.*;`. The single place the "does this need importing?" policy lives — shared by
/// the "Import class" intention (per candidate) and the auto-import-on-completion handler.
fn import_edit_for(source: &str, fqn: &str) -> Option<(usize, usize, String)> {
    let pkg = fqn.rsplit_once('.').map(|(p, _)| p).unwrap_or("");
    if pkg == "java.lang" {
        return None;
    }
    let syms = bennu_java::prelude::extract_symbols(source);
    if syms.package.as_deref() == Some(pkg) {
        return None; // same package — no import needed
    }
    if syms
        .imports
        .iter()
        .any(|i| i.star && !i.static_ && i.path.trim_end_matches(".*") == pkg)
    {
        return None; // covered by a wildcard import
    }
    let edit = bennu_intentions::prelude::insert_import_edit(source, fqn)?; // None if already imported
    Some((edit.start, edit.end, edit.replacement))
}

/// Args for [`bennu_import_edit`] — the auto-import-on-completion query.
#[derive(Deserialize)]
pub struct ImportEditArgs {
    /// The current buffer text (the FE passes the doc AFTER inserting the accepted name; the import
    /// region is above the caret, so its offsets are unaffected).
    pub source: String,
    /// The fully-qualified name to import (from the accepted completion item's `auto_import`).
    pub fqn: String,
}

/// A ready import edit: a byte range in `source` to replace with `replacement` (an insertion, so
/// `start == end`). `null`-shaped absent result when no import is needed.
#[derive(Serialize)]
pub struct ImportEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Compute the `import <fqn>;` edit for `source`, or nothing when no import is needed. Pure (parses
/// the buffer, no index) — the FE calls this when a type-name completion with `auto_import` is
/// accepted and the auto-import setting is on.
#[arbor_rpc::handler]
fn bennu_import_edit(_ctx: &BennuState, args: ImportEditArgs) -> Result<Option<ImportEdit>, String> {
    Ok(import_edit_for(&args.source, &args.fqn)
        .map(|(start, end, replacement)| ImportEdit { start, end, replacement }))
}

#[cfg(test)]
mod tests {
    use super::import_edit_for;

    /// The applied source after inserting `fqn`'s import (or `None` when no import is needed).
    fn applied(source: &str, fqn: &str) -> Option<String> {
        import_edit_for(source, fqn)
            .map(|(s, e, r)| format!("{}{}{}", &source[..s], r, &source[e..]))
    }

    #[test]
    fn adds_an_import_for_a_normal_type() {
        let src = "package a;\n\nclass C { List x; }\n";
        let out = applied(src, "java.util.List").expect("edit");
        assert!(out.contains("import java.util.List;"), "{out}");
    }

    #[test]
    fn no_import_for_java_lang() {
        assert!(import_edit_for("package a;\nclass C { String s; }\n", "java.lang.String").is_none());
    }

    #[test]
    fn no_import_for_same_package() {
        // `Helper` is in the file's own package `a` → no import needed.
        assert!(import_edit_for("package a;\nclass C { Helper h; }\n", "a.Helper").is_none());
    }

    #[test]
    fn no_import_when_star_imported() {
        let src = "package a;\nimport java.util.*;\nclass C { List x; }\n";
        assert!(import_edit_for(src, "java.util.List").is_none());
    }

    #[test]
    fn no_import_when_already_imported() {
        let src = "package a;\nimport java.util.List;\nclass C { List x; }\n";
        assert!(import_edit_for(src, "java.util.List").is_none());
    }
}
