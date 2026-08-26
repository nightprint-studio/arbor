//! The seam a language plugs into — and the two ways a language can supply its declarations.
//!
//! Everything above this file is written against [`Target`]: the conventions, the config, the
//! violations, the fix. A pack only has to answer *which declarations are in this file, and what
//! kind is each one*. There are two ways to answer, and they are not equally good:
//!
//! * [`DeclSource::Grammar`] — parsed here, from a tree-sitter grammar. Exact, offline, and it
//!   sees **everything**: locals, parameters, type parameters, package segments. This is what Java
//!   uses, because Bennu's Java engine is its own and there is no server to ask.
//!
//! * [`DeclSource::Symbols`] — taken from a language server's document outline
//!   (`textDocument/documentSymbol`), which Bennu already speaks for every language it routes to a
//!   server. Costs no new grammar and covers a whole family of languages at once — but an outline
//!   is an *outline*: it lists types and their members, and **no server reports locals or
//!   parameters**. Those targets simply never fire, and the pack says so rather than pretending.
//!   It also needs the server installed and warm; with none, there is nothing to check.
//!
//! A pack is therefore data — id, label, extensions, the standard its community uses, and where
//! its declarations come from — and only the grammar half needs behaviour ([`GrammarWalk`]).
//! Adding a server-backed language is a row in [`PACKS`]; adding a grammar-backed one is a module.
//!
//! ## The safety of a fix is not a property of the target alone
//!
//! [`Target::is_file_local`] says a local or a parameter cannot be referred to from outside its
//! file. That is true of a declaration a *grammar* found. It is **not** true of one an outline
//! reported: an outline only contains top-level and member declarations, so a TypeScript module
//! binding reported as a `variable` is exactly the kind of thing another file imports. Combining
//! the two is [`Pack::fix_is_file_local`], and every caller goes through it — getting this wrong
//! would silently rename an exported symbol across a project with no preview.

use tree_sitter::{Language, Node};

use crate::convention::Convention;
use crate::target::Target;

/// One declaration a pack found: what it is, what it is called, and where the *name* sits.
///
/// The span is the identifier alone, never the whole declaration — it is what gets underlined and
/// what a rename replaces, and a squiggle under an entire method body would be unreadable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
    pub target: Target,
    pub name: String,
    /// Start byte offset of the identifier in the source.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
}

/// Where a pack's declarations come from. See the module doc — the difference is not an
/// implementation detail, it changes which targets can fire and how a fix may be applied.
pub enum DeclSource {
    /// Parsed here, from a grammar.
    Grammar(&'static dyn GrammarWalk),
    /// From a language server's document outline: pairs of (the server's lowercase symbol kind,
    /// the target it plays). A kind that is not listed is not checked.
    Symbols(&'static [(&'static str, Target)]),
}

/// The behaviour half of a grammar-backed pack.
pub trait GrammarWalk: Send + Sync {
    /// The grammar that reads this language.
    fn language(&self) -> Language;
    /// Every declaration `node` introduces, appended to `out`. Most nodes introduce none; a
    /// `int a_b, c_d;` introduces two.
    fn declarations(&self, node: Node, source: &str, out: &mut Vec<Declared>);
}

/// A language's contribution to the pack.
pub struct Pack {
    /// Stable id — the `[naming.rules.<id>]` key. Never rename one: it is a config key that lives
    /// in users' repositories.
    pub id: &'static str,
    /// How a settings screen names the language.
    pub label: &'static str,
    /// Lower-cased extensions, without the dot, that this pack claims.
    pub extensions: &'static [&'static str],
    /// The conventions this language's community actually uses — what the "adopt the standard
    /// convention" action fills in. Never applied on its own: the default config is off, and this
    /// is only ever what the user is *offered*.
    pub standard: &'static [(Target, Convention)],
    pub source: DeclSource,
}

impl Pack {
    /// Whether this pack can ever report `target`.
    ///
    /// A grammar sees every declaration in the file. An outline sees the kinds the server reports,
    /// so a target no kind maps to can never fire — and a settings screen showing it as
    /// configurable would be offering a rule that does nothing.
    pub fn supports(&self, target: Target) -> bool {
        match &self.source {
            DeclSource::Grammar(_) => true,
            DeclSource::Symbols(kinds) => kinds.iter().any(|(_, t)| *t == target),
        }
    }

    /// Whether a fix for `target` can be applied without showing the user what it touches.
    ///
    /// Only for a declaration a **grammar** found, and only for a target whose references cannot
    /// leave the file. An outline reports top-level and member declarations — a `variable` in one
    /// is a module binding another file may well import — so nothing sourced from symbols is ever
    /// treated as file-local. See the module doc.
    pub fn fix_is_file_local(&self, target: Target) -> bool {
        matches!(self.source, DeclSource::Grammar(_)) && target.is_file_local()
    }

    /// Whether this pack's declarations come from a language server's outline rather than a
    /// grammar — which decides both which scan entry point applies and whether an outline is worth
    /// fetching at all.
    pub fn is_symbol_backed(&self) -> bool {
        matches!(self.source, DeclSource::Symbols(_))
    }

    /// The target a server's symbol `kind` plays here, if any. Always `None` for a grammar pack.
    pub fn target_for_kind(&self, kind: &str) -> Option<Target> {
        match &self.source {
            DeclSource::Grammar(_) => None,
            DeclSource::Symbols(kinds) => {
                kinds.iter().find(|(k, _)| *k == kind).map(|(_, t)| *t)
            }
        }
    }
}

/// Every compiled-in pack.
///
/// Go and C++ are deliberately absent: their conventions are not a function of the declaration
/// alone. Go spells the same kind of thing `Foo` or `foo` depending on whether it is exported, and
/// C++ has no single community convention — a rule that cannot decide from the declaration would
/// report false positives, and a naming check that cries wolf is one nobody leaves switched on.
pub static PACKS: &[Pack] = &[
    crate::java::JAVA,
    crate::symbols::TYPESCRIPT,
    crate::symbols::JAVASCRIPT,
    crate::symbols::RUST,
];

/// Every compiled-in pack.
pub fn packs() -> &'static [Pack] {
    PACKS
}

/// The pack that claims `path` by extension, if any.
pub fn pack_for_path(path: &str) -> Option<&'static Pack> {
    if !path.contains('.') {
        return None;
    }
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    packs().iter().find(|p| p.extensions.contains(&ext.as_str()))
}

/// The text of `node`, or `""` when it is not valid UTF-8 (which the grammar makes impossible for
/// an identifier, but the API is fallible and a panic here would take down a validation pass).
pub(crate) fn text<'a>(node: Node, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or("")
}

/// Push the `name`-field identifier of `node` as `target`, if it has one.
pub(crate) fn push_named(node: Node, source: &str, target: Target, out: &mut Vec<Declared>) {
    let Some(name) = node.child_by_field_name("name") else { return };
    push_node(name, source, target, out);
}

/// Push `node` itself as the identifier of a `target` declaration.
pub(crate) fn push_node(node: Node, source: &str, target: Target, out: &mut Vec<Declared>) {
    let text = text(node, source);
    if text.is_empty() {
        return;
    }
    out.push(Declared {
        target,
        name: text.to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions_are_claimed_by_exactly_one_pack() {
        let mut seen = std::collections::BTreeSet::new();
        for pack in packs() {
            for ext in pack.extensions {
                assert!(
                    seen.insert(*ext),
                    "`{ext}` is claimed by more than one pack — the first would silently win"
                );
                assert!(!ext.starts_with('.'), "`{ext}` must not carry its dot");
                assert_eq!(*ext, ext.to_ascii_lowercase(), "`{ext}` must be lower-cased");
            }
        }
    }

    #[test]
    fn resolves_a_pack_by_extension_only() {
        assert_eq!(pack_for_path("src/Foo.java").map(|p| p.id), Some("java"));
        assert_eq!(pack_for_path("src/Foo.JAVA").map(|p| p.id), Some("java"));
        assert_eq!(pack_for_path("src/app.ts").map(|p| p.id), Some("typescript"));
        assert_eq!(pack_for_path("src/main.rs").map(|p| p.id), Some("rust"));
        assert!(pack_for_path("src/messages.properties").is_none());
        assert!(pack_for_path("src/page.jsp").is_none());
        // No extension at all is not a match on the whole file name.
        assert!(pack_for_path("java").is_none());
    }

    #[test]
    fn a_pack_only_offers_a_standard_for_what_it_can_report() {
        for pack in packs() {
            assert!(!pack.standard.is_empty(), "{} offers no standard", pack.id);
            for (target, _) in pack.standard {
                assert!(
                    pack.supports(*target),
                    "{} offers a standard for {target}, which it can never report",
                    pack.id
                );
            }
        }
    }

    #[test]
    fn only_a_grammar_pack_ever_applies_a_fix_without_asking() {
        let java = pack_for_path("A.java").expect("java pack");
        assert!(java.fix_is_file_local(Target::Local));
        assert!(java.fix_is_file_local(Target::Parameter));
        assert!(!java.fix_is_file_local(Target::Method));

        // An outline reports top-level and member declarations, so nothing it contains is safe to
        // rename unseen — not even something it calls a variable.
        let ts = pack_for_path("a.ts").expect("typescript pack");
        for target in Target::ALL {
            assert!(!ts.fix_is_file_local(target), "{target} must not be file-local from an outline");
        }
    }

    #[test]
    fn symbol_packs_never_claim_locals_or_parameters() {
        for pack in packs() {
            if matches!(pack.source, DeclSource::Grammar(_)) {
                continue;
            }
            for target in [Target::Parameter, Target::Local] {
                assert!(
                    !pack.supports(target),
                    "{} maps a kind to {target}, which an outline does not contain — a rule set \
                     there would silently never fire",
                    pack.id
                );
            }
        }
    }
}
