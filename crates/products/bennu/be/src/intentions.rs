//! `intentions` domain — `bennu_intentions_at`: every Alt+Enter quick-fix applicable at the caret.
//!
//! One handler over the whole `bennu-intentions` catalog: it returns the applicable transforms as
//! offers (id + label + edit), so the editor makes a single round-trip per Alt+Enter and adding an
//! intention is a change in the pure crate only. Replaces the old per-transform handlers.

use bennu_core::prelude::BennuState;
use bennu_refactor::prelude::{EditSelection, RefactorEdit};
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
    /// The diagnostics the editor is currently showing, so the ones under the caret can be offered
    /// their **fix**.
    ///
    /// Passed in rather than recomputed. The editor has them — it drew them — and revalidating the
    /// file to answer an Alt+Enter would run every check in it for the sake of the one squiggle the
    /// caret is on. What arrives is only `code` and a span; the fixes read the source themselves and
    /// never the message (see `bennu_intentions::prelude::fixes_for`).
    #[serde(default)]
    pub diagnostics: Vec<DiagRef>,
}

/// A diagnostic as a quick-fix needs it: what kind, and where.
#[derive(Deserialize, Clone)]
pub struct DiagRef {
    /// The stable kind slug — `unused-import`, `unhandled-checked-exception`.
    pub code: String,
    /// Byte span in `source`.
    pub start: usize,
    pub end: usize,
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
    /// The whole edit, when it touches more than one place — a framework rewrite and the import it
    /// needs. `start`/`end`/`replacement` then repeat the first of them, so a reader that knows only
    /// the single-range shape still sees where the offer is. Empty (and not sent) for every other
    /// offer.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<EditWire>,
    /// What the editor selects once the edits are applied — a placeholder the offer wrote, so the
    /// next keystroke replaces it. Bytes into the inserted text of ONE edit: `edits[edit]`, or, for a
    /// single-range offer (`edits` empty), `replacement` as edit `0`. Not a document offset, which
    /// would be ambiguous about whether it was measured before or after the edits (see
    /// [`EditSelection`]). Absent — and not sent — for an offer that leaves the caret where the
    /// editor puts it, so a reader that never learned the field applies every offer as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<EditSelection>,
}

/// One replacement of a multi-place offer — see [`OfferWire::edits`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EditWire {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Which section of the popup an offer belongs to. The editor reads them in this order, because
/// that is the order of why Alt+Enter gets pressed.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfferCategory {
    /// Repairs a problem under the caret — an import, a missing method, a package mismatch.
    Fix,
    /// Changes code that is already right: a simplification, a rewrite.
    Intention,
    /// Opens a generator that writes members. The least specific thing that can be offered, so it
    /// is only offered where a member is being written (see [`member_site`]).
    Generate,
}

/// An offer as the editor receives it: the offer itself, its section, and whether it is the one to
/// take. Flattened, so the wire shape is the offer's plus two fields.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RankedOffer {
    #[serde(flatten)]
    pub offer: OfferWire,
    pub category: OfferCategory,
    /// The candidate the editor may mark as the suggestion — today the nearest of several imports,
    /// and only when a rule rather than the alphabet put it first.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub preferred: bool,
}

impl OfferWire {
    fn in_section(self, category: OfferCategory) -> RankedOffer {
        RankedOffer { offer: self, category, preferred: false }
    }
}

/// Every intention applicable at the caret (empty when none fits).
#[arbor_rpc::handler]
fn bennu_intentions_at(_ctx: &BennuState, args: IntentionsArgs) -> Result<Vec<RankedOffer>, String> {
    let mut offers: Vec<RankedOffer> = Vec::new();

    // Everything below this point up to the naming offer is **Java**, and is now guarded as such:
    // the naming pack covers server-backed languages too, so the editor asks this handler for a
    // `.ts` and a `.rs` as well. Running Java source transforms over TypeScript would offer edits
    // computed from a grammar that never read it.
    if crate::intel::is_java_file(&args.file) {
        // The pure source transforms (caret-anchored).
        offers.extend(
            bennu_intentions::prelude::intentions_at(&args.source, args.offset).into_iter().map(
                |o| {
                    OfferWire {
                        id: o.id,
                        label: o.label,
                        start: o.start,
                        end: o.end,
                        replacement: o.replacement,
                        action: None,
                        edits: Vec::new(),
                        select: None,
                    }
                    .in_section(OfferCategory::Intention)
                },
            ),
        );
        offers.extend(java_file_offers(&args));
        offers.extend(quick_fix_offers(&args).into_iter().map(|o| o.in_section(OfferCategory::Fix)));
        // A blank `final` field: offered on its whole declaration, squiggle or not — see the module.
        offers.extend(crate::final_field_fixes::final_field_offers(&args));
    }

    // What the project's frameworks offer here: the fixes for their own diagnostics under the caret,
    // and the rewrites that need no diagnostic. Outside the Java guard on purpose — an extension
    // decides for itself which files it answers for, and a framework with an opinion about an XML
    // file should not need this handler to learn about it.
    offers.extend(framework_offers(&args));

    // The caret is on a declaration whose name breaks the project's naming convention. The fix is
    // the name the convention itself produced, so the offer never has to compute one.
    //
    // Dispatched as a **rename action**, not as an edit — even for a local, where replacing the
    // identifier in place would rewrite the declaration and leave every use of it behind. The
    // project's semantic engine is what knows the others.
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
            edits: Vec::new(),
            select: None,
        }
        .in_section(OfferCategory::Fix));
    }

    Ok(settle(offers))
}

/// The one rule that looks at the whole list: a caret on a problem is not a place to be offered a
/// generator.
///
/// On an unimported type, "Generate constructor…" is a row about something else entirely, sitting
/// between the user and the import they pressed the key for. The generators stay one keystroke away
/// (Alt+Insert) wherever they are dropped from here.
fn settle(mut offers: Vec<RankedOffer>) -> Vec<RankedOffer> {
    if offers.iter().any(|o| o.category == OfferCategory::Fix) {
        offers.retain(|o| o.category != OfferCategory::Generate);
    }
    offers
}

/// The diagnostics under the caret: its span contains it, or the caret is at either end of it — a
/// squiggle you have just walked the caret onto is the one you want to fix, and "inside" is a
/// distinction nobody makes while pressing Alt+Enter.
pub(crate) fn under_caret(args: &IntentionsArgs) -> impl Iterator<Item = &DiagRef> {
    args.diagnostics.iter().filter(move |d| args.offset >= d.start && args.offset <= d.end)
}

/// An offer that opens something rather than editing — its only payload is where it was asked.
fn action_offer(id: &str, label: &str, at: usize) -> OfferWire {
    OfferWire {
        id: id.to_string(),
        label: label.to_string(),
        start: at,
        end: at,
        replacement: String::new(),
        action: Some(id.to_string()),
        edits: Vec::new(),
        select: None,
    }
}

/// An edit offer: the first edit repeated in the single-range fields, the whole list only when there
/// is more than one (see [`OfferWire::edits`]). `select` indexes `edits` in the order given, which is
/// the order the wire keeps — and a single edit travelling in the single-range fields is edit `0`,
/// so a transform's selection indexes the offer unchanged.
///
/// The one place an offer with a selection is built, so the guard below is written once.
pub(crate) fn offer_of(id: &str, label: &str, edits: Vec<EditWire>, select: Option<EditSelection>) -> OfferWire {
    let first = edits.first().cloned().unwrap_or(EditWire { start: 0, end: 0, text: String::new() });
    // A selection pointing past the list would select nothing the editor can find: dropped here,
    // so the wire never carries one.
    let select = select.filter(|s| {
        edits.get(s.edit).is_some_and(|e| s.start <= s.end && s.end <= e.text.len())
    });
    OfferWire {
        id: id.to_string(),
        label: label.to_string(),
        start: first.start,
        end: first.end,
        replacement: first.text,
        action: None,
        edits: if edits.len() > 1 { edits } else { Vec::new() },
        select,
    }
}

/// A transform's edit, as the wire carries it.
pub(crate) fn edit_wire(edit: &RefactorEdit) -> EditWire {
    EditWire { start: edit.start, end: edit.end, text: edit.text.clone() }
}

/// The **fixes** for the diagnostics under the caret (see [`under_caret`]).
///
/// Text-only fixes come from the pure crate; the ones that need types come from the resolver, and
/// only if the project has one built. A cold index simply offers fewer fixes.
fn quick_fix_offers(args: &IntentionsArgs) -> Vec<OfferWire> {
    let under: Vec<&DiagRef> = under_caret(args).collect();
    if under.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for d in &under {
        out.extend(
            bennu_intentions::prelude::fixes_for(&d.code, &args.source, d.start, d.end)
                .into_iter()
                .map(|f| OfferWire {
                    id: f.id,
                    label: f.label,
                    start: f.start,
                    end: f.end,
                    replacement: f.replacement,
                    action: None,
                    edits: Vec::new(),
                    select: None,
                }),
        );
        // Tree-only fixes for a diagnostic the resolver raised: offered whether or not an index is
        // built, because the question they would need it for has already been answered.
        out.extend(crate::quick_fix::tree_fixes(&d.code, &args.source, d.start, d.end));
    }
    if let Some(resolver) = IndexService::global().caret_resolver_for(&args.file) {
        for d in &under {
            out.extend(crate::quick_fix::resolver_fixes(
                &d.code,
                &args.source,
                d.start,
                d.end,
                &*resolver,
            ));
        }
    }
    out
}

/// The offers the project's framework extensions make at the caret, as wire offers.
///
/// Only the diagnostics **under the caret** are handed over, by the same rule the language's own fixes
/// use — an extension that saw every squiggle in the file would offer to fix one three screens away.
/// A multi-place offer keeps its first edit in the single-range fields as well, so the offer still
/// reads as being somewhere.
///
/// An offer is a **fix** when a problem under the caret comes from the same extension — codes and
/// intention ids are both namespaced `<extension>.<name>` — and an intention otherwise: an AssertJ
/// rewrite of a JUnit assertion repairs nothing.
fn framework_offers(args: &IntentionsArgs) -> Vec<RankedOffer> {
    let problems: Vec<bennu_ext::prelude::ExtProblem> = under_caret(args)
        .map(|d| bennu_ext::prelude::ExtProblem { code: d.code.clone(), start: d.start, end: d.end })
        .collect();
    let namespace = |id: &str| id.rsplit_once('.').map(|(ns, _)| ns.to_string());
    crate::frameworks::intentions_for(&args.file, &args.source, args.offset, &problems)
        .into_iter()
        .filter_map(|intention| {
            let first = intention.edits.first()?.clone();
            let fixes = namespace(&intention.id)
                .is_some_and(|ns| problems.iter().any(|p| namespace(&p.code).as_deref() == Some(&*ns)));
            let offer = OfferWire {
                id: intention.id,
                label: intention.label,
                start: first.start,
                end: first.end,
                replacement: first.text,
                action: None,
                edits: intention
                    .edits
                    .into_iter()
                    .map(|e| EditWire { start: e.start, end: e.end, text: e.text })
                    .collect(),
                select: None,
            };
            Some(offer.in_section(if fixes { OfferCategory::Fix } else { OfferCategory::Intention }))
        })
        .collect()
}

/// The Java offers that need more than the text at the caret: the file's location (a package or a
/// name mismatch), the class index (importing a type), the hierarchy (overriding) — and the
/// generators, which need to know whether the caret is somewhere a member is written.
fn java_file_offers(args: &IntentionsArgs) -> Vec<RankedOffer> {
    let (file, source, offset) = (args.file.as_str(), args.source.as_str(), args.offset);
    let mut fixes: Vec<OfferWire> = Vec::new();
    let mut offers: Vec<RankedOffer> = Vec::new();

    // File-context intentions on a package mismatch: (a) rewrite the declaration to match the folder,
    // or (b) move the file to the folder matching the declaration. Both anchored on the same mismatch.
    if let Some(expected) = std::path::Path::new(file)
        .parent()
        .and_then(bennu_java::prelude::infer_package)
    {
        if let Some((start, end, replacement)) =
            bennu_check::prelude::change_package(source, &expected)
        {
            fixes.push(OfferWire {
                id: "change-package".to_string(),
                label: format!("Set package to `{expected}`"),
                start,
                end,
                replacement,
                action: None,
                edits: Vec::new(),
                select: None,
            });
            // The move alternative — a filesystem action, not an edit (dispatched by the editor).
            if let Some(declared) = bennu_java::prelude::extract_symbols(source).package {
                fixes.push(OfferWire {
                    id: "move-to-package".to_string(),
                    label: format!("Move file to package `{declared}`"),
                    start: 0,
                    end: 0,
                    replacement: String::new(),
                    action: Some("move-to-package".to_string()),
                    edits: Vec::new(),
                    select: None,
                });
            }
        }
    }

    // The file's name and the type's name disagree — which for a `public` top-level type is not a
    // matter of taste: JLS §7.6 ties the two, so the file does not compile until one of them moves.
    //
    // Two offers, because there is no way to know which name is the right one. `Foo.java` holding
    // `public class Bar` happens both ways round — someone renamed the class in a text editor, and
    // someone copied a file and renamed the class inside it — and an editor that picks for you gets
    // it wrong half the time, silently, in the direction that loses the name you meant to keep.
    //
    // Neither is a plain edit. Renaming the type has to carry every use of it (a `replacement` in
    // this buffer would leave the rest of the project calling a name that no longer exists), so it
    // goes through the rename engine's preview. Renaming the file is a filesystem move, which the
    // editor performs.
    fixes.extend(name_mismatch_offers(file, source));
    offers.extend(fixes.into_iter().map(|o| o.in_section(OfferCategory::Fix)));

    // "Import class": the caret is on a bare, unimported type name → offer to add its import, one
    // offer per candidate FQN (the Alt+Enter menu is the "which import?" picker the user asked for).
    if let Some(simple) = bennu_java::prelude::simple_type_needing_import(source, offset) {
        offers.extend(import_class_offers(file, source, &simple));
    }

    // The generators, offered where a member is being written — in a class body, on a field, on the
    // class's own header — and nowhere else. Inside a method, on an argument, on a type that does
    // not resolve, "Generate constructor…" has nothing to do with the caret and pushes the row that
    // does further down. `settle` drops them again wherever a fix is on offer.
    let site = member_site(source, offset);
    if site.is_some() {
        offers.push(
            action_offer("generate-constructor", "Generate constructor…", offset)
                .in_section(OfferCategory::Generate),
        );
    }
    if site.as_ref().is_some_and(|s| s.has_instance_fields) {
        offers.push(
            action_offer("generate-getters-setters", "Generate getters and setters…", offset)
                .in_section(OfferCategory::Generate),
        );
    }

    // "Implement / override methods": offered only when there IS something to override, which is
    // why it asks rather than offering unconditionally. An Alt+Enter item that opens a dialog
    // saying "nothing here" is an item that teaches you to stop reading the menu.
    //
    // A dialog, not an edit — the user picks which methods — so it travels as an action and the
    // editor opens the picker. On a class the checker says leaves an abstract method unimplemented
    // it is that problem's FIX, and offered on the squiggle; otherwise it is a generator, and
    // offered where the others are. The hierarchy walk only runs in one of those two places.
    let fixes_abstract = under_caret(args).any(|d| d.code == "missing-abstract-method");
    if (fixes_abstract || site.is_some())
        && !IndexService::global().overridable_at(file, source, offset).is_empty()
    {
        let category = if fixes_abstract { OfferCategory::Fix } else { OfferCategory::Generate };
        offers.push(
            action_offer("override-methods", "Implement / override methods…", offset).in_section(category),
        );
    }
    offers
}

/// A place a generator can write into, and what it would find there.
struct MemberSite {
    /// Whether the class declares a field that is not `static` — without one there is nothing to
    /// write an accessor for.
    has_instance_fields: bool,
}

/// The class or enum whose members the caret is among: in its body, on one of its fields, on its
/// header. `None` inside anything with a body of its own — a method, a constructor, an initializer,
/// a lambda, an anonymous class — and for an interface, an annotation type or a record, which have
/// no constructor to generate and no fields to wrap.
fn member_site(source: &str, offset: usize) -> Option<MemberSite> {
    let tree = bennu_java::prelude::parse_java(source)?;
    let root = tree.root_node();
    let at = offset.min(source.len());
    let mut cur = root.named_descendant_for_byte_range(at, (at + 1).min(source.len()));
    while let Some(node) = cur {
        match node.kind() {
            "method_declaration"
            | "constructor_declaration"
            | "compact_constructor_declaration"
            | "static_initializer"
            | "block"
            | "lambda_expression"
            | "object_creation_expression"
            | "interface_declaration"
            | "annotation_type_declaration"
            | "record_declaration" => return None,
            "class_declaration" | "enum_declaration" => {
                return Some(MemberSite { has_instance_fields: has_instance_fields(&node, source) });
            }
            _ => cur = node.parent(),
        }
    }
    None
}

/// Whether `type_decl` declares a non-`static` field — in its body, or for an enum after its
/// constants.
fn has_instance_fields(type_decl: &tree_sitter::Node, source: &str) -> bool {
    let Some(body) = type_decl.child_by_field_name("body") else { return false };
    let mut containers = vec![body];
    let mut cursor = body.walk();
    containers.extend(body.named_children(&mut cursor).filter(|n| n.kind() == "enum_body_declarations"));
    let is_static = |field: &tree_sitter::Node| {
        let mut c = field.walk();
        let found = field.named_children(&mut c).any(|m| {
            m.kind() == "modifiers"
                && m.utf8_text(source.as_bytes()).is_ok_and(|t| t.split_whitespace().any(|w| w == "static"))
        });
        found
    };
    containers.iter().any(|container| {
        let mut c = container.walk();
        let found = container
            .named_children(&mut c)
            .any(|member| member.kind() == "field_declaration" && !is_static(&member));
        found
    })
}

/// The two ways to settle a public type whose name disagrees with its file's — see the call site.
///
/// Silent for a file with no `.java` stem to compare against, and for the second and later
/// mismatches in one file: two `public` top-level types is a different error, and offering to
/// rename the file after each of them in turn would be offering to make the same file right for one
/// of them and wrong for the other.
fn name_mismatch_offers(file: &str, source: &str) -> Vec<OfferWire> {
    let Some(stem) = std::path::Path::new(file).file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    let Some(tree) = bennu_java::prelude::parse_java(source) else { return Vec::new() };
    let found = bennu_check::prelude::type_file_mismatches(tree.root_node(), source, stem);
    let [mismatch] = found.as_slice() else { return Vec::new() };

    vec![
        OfferWire {
            id: "rename-type-to-file".to_string(),
            label: format!("Rename {} `{}` to `{stem}` (match the file)", mismatch.keyword, mismatch.name),
            start: mismatch.start,
            end: mismatch.end,
            // The payload the rename action plans with: the caret goes to `start`, the new name is
            // the file's own stem.
            replacement: stem.to_string(),
            action: Some("rename-symbol-preview".to_string()),
            edits: Vec::new(),
            select: None,
        },
        OfferWire {
            id: "rename-file-to-type".to_string(),
            label: format!("Rename the file to `{}.java` (match the {})", mismatch.name, mismatch.keyword),
            start: mismatch.start,
            end: mismatch.end,
            // The payload the file action moves to: a base name, never a path — the file stays in
            // its package, and a package move is the OTHER intention.
            replacement: format!("{}.java", mismatch.name),
            action: Some("rename-file".to_string()),
            edits: Vec::new(),
            select: None,
        },
    ]
}

/// Build the "Import `<fqn>`" offers for the unimported simple type `simple` used in `source`: one per
/// candidate FQN from the project's class-name index, **nearest first**, minus those that need NO
/// import (see [`import_edit_for`]). Each carries the edit that inserts the `import …;` line. Capped
/// so a pathologically common name can't flood the menu.
fn import_class_offers(file: &str, source: &str, simple: &str) -> Vec<RankedOffer> {
    const MAX_CANDIDATES: usize = 25;

    let choices = IndexService::global().import_choices(file, source, simple);
    let mut out = Vec::new();
    for (place, fqn) in choices.fqns.iter().enumerate() {
        let Some((start, end, replacement)) = import_edit_for(source, fqn) else {
            continue; // needs no import (java.lang / same package / star / already imported)
        };
        out.push(RankedOffer {
            offer: OfferWire {
                id: format!("import-class:{fqn}"),
                label: format!("Import '{fqn}'"),
                start,
                end,
                replacement,
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

/// The edit that adds `import <fqn>;` to `source` (byte range + replacement), or `None` when no import
/// is needed: the fqn is a `java.lang` type, in the file's OWN package, already imported, or covered
/// by an `import pkg.*;`. The single place the "does this need importing?" policy lives — shared by
/// the "Import class" intention (per candidate) and the auto-import-on-completion handler.
pub(crate) fn import_edit_for(source: &str, fqn: &str) -> Option<(usize, usize, String)> {
    // "Does this need importing" is "is the name already in scope", and that question has one
    // answer in the workspace — the one the validator reports a missing import with. Asking it
    // separately here is how the two could come to disagree: an intention offering an import the
    // check never asked for, or refusing one it did.
    let syms = bennu_java::prelude::extract_symbols(source);
    let simple = fqn.rsplit('.').next().unwrap_or(fqn);
    let binary = fqn.replace('.', "/");
    if bennu_java::prelude::simple_name_reaches(&binary, simple, syms.package.as_deref(), &syms.imports) {
        return None;
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
    use super::{
        import_edit_for, member_site, name_mismatch_offers, offer_of, settle, EditWire, OfferCategory,
        OfferWire,
    };
    use bennu_refactor::prelude::EditSelection;

    // ── how an edit offer travels ────────────────────────────────────────────────────────────

    fn edit(start: usize, text: &str) -> EditWire {
        EditWire { start, end: start, text: text.to_string() }
    }

    #[test]
    fn a_single_edit_travels_in_the_single_range_fields_only() {
        let offer = offer_of("initialize-variable", "Initialize variable 'client'", vec![edit(40, " = null")], None);
        assert_eq!((offer.start, offer.replacement.as_str()), (40, " = null"));
        assert!(offer.edits.is_empty());
        assert!(offer.select.is_none());
    }

    #[test]
    fn several_edits_travel_whole_and_repeat_the_first() {
        let offer = offer_of("add-constructor-parameter", "Add constructor parameter", vec![edit(90, "b"), edit(30, "a")], None);
        assert_eq!(offer.start, 90);
        assert_eq!(offer.edits.len(), 2);
    }

    /// On a single-range offer, edit `0` is `replacement` — the selection names the placeholder there.
    #[test]
    fn a_selection_travels_with_the_offer_and_points_into_its_replacement() {
        let select = EditSelection { edit: 0, start: 3, end: 7 };
        let offer = offer_of("initialize-variable", "Initialize variable 'client'", vec![edit(40, " = null")], Some(select));
        assert_eq!(offer.select, Some(select));
        assert_eq!(&offer.replacement[select.start..select.end], "null");
        let json = serde_json::to_value(&offer).unwrap();
        assert_eq!(json["select"], serde_json::json!({ "edit": 0, "start": 3, "end": 7 }));
    }

    #[test]
    fn a_selection_that_points_past_its_edit_is_not_sent() {
        let past_the_list = EditSelection { edit: 1, start: 0, end: 1 };
        let past_the_text = EditSelection { edit: 0, start: 3, end: 99 };
        for select in [past_the_list, past_the_text] {
            let offer = offer_of("initialize-variable", "x", vec![edit(40, " = null")], Some(select));
            assert!(offer.select.is_none(), "{select:?}");
            assert!(serde_json::to_value(&offer).unwrap().get("select").is_none());
        }
    }

    // ── where the generators are offered ─────────────────────────────────────────────────────

    fn site(source: &str, marker: &str) -> Option<bool> {
        member_site(source, source.find(marker).expect("marker")).map(|s| s.has_instance_fields)
    }

    #[test]
    fn the_generators_belong_in_a_class_body() {
        assert_eq!(site("class A {\n    private String name;\n    /*here*/\n}\n", "/*here*/"), Some(true));
    }

    #[test]
    fn a_caret_inside_a_method_is_not_writing_a_member() {
        let src = "class A {\n    String name;\n    void f() {\n        List x = null;\n    }\n}\n";
        assert_eq!(site(src, "List"), None);
    }

    #[test]
    fn a_class_with_only_static_fields_has_nothing_to_get() {
        assert_eq!(site("class A {\n    static int COUNT;\n    /*here*/\n}\n", "/*here*/"), Some(false));
    }

    #[test]
    fn an_interface_has_nothing_to_generate() {
        assert_eq!(site("interface A {\n    /*here*/\n}\n", "/*here*/"), None);
    }

    #[test]
    fn a_fix_on_offer_sends_the_generators_away() {
        let offer = |id: &str| OfferWire {
            id: id.to_string(),
            label: id.to_string(),
            start: 0,
            end: 0,
            replacement: String::new(),
            action: None,
            edits: Vec::new(),
            select: None,
        };
        let with_fix = settle(vec![
            offer("import-class:java.util.List").in_section(OfferCategory::Fix),
            offer("generate-constructor").in_section(OfferCategory::Generate),
        ]);
        assert_eq!(with_fix.iter().map(|o| o.offer.id.as_str()).collect::<Vec<_>>(), ["import-class:java.util.List"]);
        let without = settle(vec![offer("generate-constructor").in_section(OfferCategory::Generate)]);
        assert_eq!(without.len(), 1);
    }

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
    // ── the file name and the type name disagree ────────────────────────────────────────────

    fn mismatch(file: &str, source: &str) -> Vec<(String, String, String)> {
        name_mismatch_offers(file, source)
            .into_iter()
            .map(|o| (o.id, o.replacement, o.action.unwrap_or_default()))
            .collect()
    }

    #[test]
    fn both_directions_are_offered_and_neither_is_chosen() {
        let got = mismatch("/p/src/main/java/a/Bar.java", "package a;\npublic class Foo {}\n");
        assert_eq!(
            got,
            vec![
                ("rename-type-to-file".into(), "Bar".into(), "rename-symbol-preview".into()),
                ("rename-file-to-type".into(), "Foo.java".into(), "rename-file".into()),
            ]
        );
    }

    #[test]
    fn a_file_that_already_agrees_offers_nothing() {
        assert!(mismatch("/p/a/Foo.java", "package a;\npublic class Foo {}\n").is_empty());
    }

    #[test]
    fn a_non_public_type_is_free_to_be_called_anything() {
        assert!(mismatch("/p/a/Bar.java", "package a;\nclass Foo {}\n").is_empty());
    }

    #[test]
    fn a_nested_type_is_named_after_nothing_in_particular() {
        let src = "package a;\npublic class Bar { public class Foo {} }\n";
        assert!(mismatch("/p/a/Bar.java", src).is_empty());
    }

    #[test]
    fn the_label_calls_the_declaration_what_it_is() {
        let offers = name_mismatch_offers("/p/a/Bar.java", "package a;\npublic enum Foo { A }\n");
        assert!(offers[0].label.contains("enum `Foo`"), "{}", offers[0].label);
        assert!(offers[1].label.contains("`Foo.java`"), "{}", offers[1].label);
    }

    #[test]
    fn two_public_types_in_one_file_get_no_offer() {
        // A second `public` top-level type is its own error, and "rename the file" would settle it
        // for one of them by breaking it for the other.
        let src = "package a;\npublic class Foo {}\npublic class Baz {}\n";
        assert!(mismatch("/p/a/Bar.java", src).is_empty());
    }

    #[test]
    fn the_file_offer_carries_a_base_name_and_never_a_path() {
        let offers = name_mismatch_offers("/p/src/main/java/a/Bar.java", "public class Foo {}\n");
        let rename_file = offers.iter().find(|o| o.id == "rename-file-to-type").unwrap();
        assert!(!rename_file.replacement.contains('/'), "{}", rename_file.replacement);
    }
}
