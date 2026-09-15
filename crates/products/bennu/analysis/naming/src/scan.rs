//! The scan: declarations in, violations out — written once, shared by every pack.
//!
//! ## Two entry points, because there are two ways to get declarations
//!
//! [`violations`] parses the file with the pack's grammar. [`violations_from_symbols`] takes a
//! language server's outline instead. Everything after that point — looking up the rule, judging
//! the name, computing the fix — is the same code, which is the whole reason
//! [`crate::pack::Pack`] models the difference as data.
//!
//! [`needs_symbols`] exists so the caller can find out whether an outline is worth fetching
//! *before* paying for the round-trip. A project that has not opted in answers `false` without
//! touching a server.
//!
//! ## The order of the guards is the performance story
//!
//! Everything that can rule a file out is asked before any work, cheapest question first. A project
//! that has not opted in — the default — answers `enabled == false` and never reaches a grammar or a
//! server; a project that opted in for methods only still skips a file whose pack has every target
//! off. Validation runs this on every keystroke's debounce and across a whole project on open, so
//! "off" has to cost nothing rather than cost one parse per file.
//!
//! ## Every violation carries its fix
//!
//! A [`Violation`] holds `suggested`, computed by the same [`Convention`] that rejected the name.
//! There is no second code path deciding what to rename to — the check and the quick-fix cannot
//! drift, because the fix *is* the reason the name was rejected.

use bennu_proto::prelude::{severity, Diagnostic, LspSymbol};
use tree_sitter::{Node, Parser};

use crate::config::{LanguageRules, NamingConfig};
use crate::convention::Convention;
use crate::pack::{pack_for_path, DeclSource, Declared, GrammarWalk, Pack};
use crate::skip::is_generated;
use crate::target::Target;

/// A name that breaks the convention its target was configured with, and the name that would not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub target: Target,
    /// The convention that rejected it — carried so a message can name the rule the user chose.
    pub convention: Convention,
    /// The name as written.
    pub name: String,
    /// The name this convention would spell it with. Never equal to `name`.
    pub suggested: String,
    /// Start byte offset of the identifier.
    pub start: usize,
    /// End byte offset (exclusive).
    pub end: usize,
    /// Whether renaming this cannot reach outside the file, so its fix may be applied without
    /// showing the user what it touches.
    ///
    /// A property of the declaration **and where it came from**, not of the target alone — see
    /// [`Pack::fix_is_file_local`]. Never true for anything an outline reported.
    pub file_local: bool,
}

impl Violation {
    /// The sentence shown in the editor and the Problems panel.
    pub fn message(&self) -> String {
        format!(
            "{} `{}` should be `{}` ({} convention)",
            self.target.label(),
            self.name,
            self.suggested,
            self.convention
        )
    }

    /// The wire diagnostic. Always the weak level: a name that breaks a convention is not a
    /// defect, and rendering it beside a genuine compile error would devalue both.
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic {
            message: self.message(),
            severity: severity::WEAK.to_string(),
            code: self.target.code().to_string(),
            start: self.start,
            end: self.end,
        }
    }
}

/// Whether this file's rules would come from a **language server's outline**, so the caller should
/// fetch one and call [`violations_from_symbols`].
///
/// `false` for a grammar-backed file, and for every file that is not going to be checked at all —
/// which is the point: it answers before anything asks a server for anything.
pub fn needs_symbols(path: &str, source: &str, config: &NamingConfig) -> bool {
    matches!(applicable(path, source, config), Some((pack, _)) if pack.is_symbol_backed())
}

/// Every violation in `source`, for a **grammar-backed** file (Java).
///
/// Empty for a server-backed file: its declarations live in an outline this function has no way to
/// fetch. Use [`needs_symbols`] + [`violations_from_symbols`] for those.
pub fn violations(path: &str, source: &str, config: &NamingConfig) -> Vec<Violation> {
    let Some((pack, rules)) = applicable(path, source, config) else { return Vec::new() };
    let DeclSource::Grammar(walk) = &pack.source else { return Vec::new() };
    check(pack, &rules, declarations_in(*walk, source))
}

/// Every violation in `symbols`, the document outline a language server produced for `source`.
pub fn violations_from_symbols(
    path: &str,
    symbols: &[LspSymbol],
    source: &str,
    config: &NamingConfig,
) -> Vec<Violation> {
    let Some((pack, rules)) = applicable(path, source, config) else { return Vec::new() };
    if !pack.is_symbol_backed() {
        return Vec::new();
    }
    check(pack, &rules, crate::symbols::declarations_from(pack, symbols, source))
}

/// The same scans, as the wire diagnostics the Problems panel and lint gutter render.
pub fn diagnostics(path: &str, source: &str, config: &NamingConfig) -> Vec<Diagnostic> {
    violations(path, source, config).iter().map(Violation::to_diagnostic).collect()
}

/// [`violations_from_symbols`], as wire diagnostics.
pub fn diagnostics_from_symbols(
    path: &str,
    symbols: &[LspSymbol],
    source: &str,
    config: &NamingConfig,
) -> Vec<Diagnostic> {
    violations_from_symbols(path, symbols, source, config)
        .iter()
        .map(Violation::to_diagnostic)
        .collect()
}

/// The violation whose identifier contains `offset`, if any — what Alt+Enter asks of a
/// grammar-backed file.
///
/// The caret counts as inside at both ends: with the caret sitting immediately after the last
/// character of a name, the name is still what the user means.
pub fn violation_at(
    path: &str,
    source: &str,
    config: &NamingConfig,
    offset: usize,
) -> Option<Violation> {
    at_offset(violations(path, source, config), offset)
}

/// [`violation_at`], for a server-backed file.
pub fn violation_at_from_symbols(
    path: &str,
    symbols: &[LspSymbol],
    source: &str,
    config: &NamingConfig,
    offset: usize,
) -> Option<Violation> {
    at_offset(violations_from_symbols(path, symbols, source, config), offset)
}

fn at_offset(found: Vec<Violation>, offset: usize) -> Option<Violation> {
    found.into_iter().find(|v| offset >= v.start && offset <= v.end)
}

/// The pack and rules that apply to this file, or `None` when there is nothing to check.
///
/// Every reason to do no work at all lives here, in the order they cost: the project has not opted
/// in, no pack claims the extension, the pack has every target off, the path is excluded, the file
/// is generated.
fn applicable(
    path: &str,
    source: &str,
    config: &NamingConfig,
) -> Option<(&'static Pack, LanguageRules)> {
    if !config.enabled {
        return None;
    }
    let pack = pack_for_path(path)?;
    // Path-aware, not pack-wide: an override can free up a rule for a subtree (test sources) or
    // tighten one, and `is_off` has to be judged on what actually applies here.
    let rules = config.rules_for_path(pack.id, path);
    if rules.is_off() || config.ignores(path) || is_generated(path, source) {
        return None;
    }
    Some((pack, rules))
}

/// Judge each declaration against the rule its target was configured with.
fn check(pack: &Pack, rules: &LanguageRules, declarations: Vec<Declared>) -> Vec<Violation> {
    let mut out = Vec::new();
    for declared in declarations {
        let convention = rules.convention_for(declared.target);
        if convention.accepts(&declared.name) {
            continue;
        }
        let Some(suggested) = convention.render(&declared.name) else { continue };
        // `accepts` is `render == name`, so this cannot fire — but a pack that ever returned an
        // empty name would otherwise produce a "rename `x` to `x`" offer.
        if suggested == declared.name {
            continue;
        }
        // A convention can spell a RESERVED WORD. `CONST` under `snake_case` is `const`, which is a
        // keyword in Java (reserved and unused, but reserved), and Apache Commons declares
        // `ObjectUtils.CONST`. Offering it produced a file that does not parse — the one outcome a
        // rename must never reach. There is nothing to suggest here, so the name is left alone
        // rather than mangled into something the user did not ask for.
        if pack.is_reserved_word(&suggested) {
            continue;
        }
        out.push(Violation {
            target: declared.target,
            convention,
            name: declared.name,
            suggested,
            start: declared.start,
            end: declared.end,
            file_local: pack.fix_is_file_local(declared.target),
        });
    }
    out
}

/// Parse `source` with `walk`'s grammar and collect every declaration it recognises.
///
/// One parse, one walk, and the pack is asked about each node exactly once — the same shape
/// `bennu-check` uses, and for the same reason: a dozen independent traversals of a 3k-line legacy
/// class is the cost that actually shows up on a project-wide pass.
pub(crate) fn declarations_in(walk: &dyn GrammarWalk, source: &str) -> Vec<Declared> {
    let mut parser = Parser::new();
    if parser.set_language(&walk.language()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else { return Vec::new() };

    let mut out = Vec::new();
    visit(tree.root_node(), &mut |node| walk.declarations(node, source, &mut out));
    out
}

/// Depth-first over every named node, applying `f`.
fn visit(node: Node, f: &mut impl FnMut(Node)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn config(pack: &str, pairs: &[(Target, Convention)]) -> NamingConfig {
        NamingConfig {
            enabled: true,
            ignore: Vec::new(),
            rules: BTreeMap::from([(
                pack.to_string(),
                LanguageRules::from_pairs(pairs.iter().copied()),
            )]),
            overrides: Vec::new(),
        }
    }

    const SRC: &str = r#"
        package com.acme;
        class order_service {
            private int item_count;
            void do_work(String the_input) { int a_local = 1; }
        }
    "#;

    #[test]
    fn reports_only_the_targets_that_were_configured() {
        let cfg = config("java", &[(Target::Method, Convention::Camel)]);
        let found = violations("src/Order.java", SRC, &cfg);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "do_work");
        assert_eq!(found[0].suggested, "doWork");
        assert_eq!(found[0].target, Target::Method);
    }

    #[test]
    fn a_disabled_config_reports_nothing() {
        let mut cfg = config("java", &[(Target::Method, Convention::Camel)]);
        cfg.enabled = false;
        assert!(violations("src/Order.java", SRC, &cfg).is_empty());
        assert!(!needs_symbols("src/app.ts", "", &cfg));
    }

    #[test]
    fn every_target_off_reports_nothing() {
        let cfg = config("java", &[(Target::Method, Convention::Any)]);
        assert!(violations("src/Order.java", SRC, &cfg).is_empty());
    }

    #[test]
    fn a_file_no_pack_claims_is_skipped() {
        let cfg = config("java", &[(Target::Method, Convention::Camel)]);
        assert!(violations("src/messages.properties", SRC, &cfg).is_empty());
        assert!(violations("web/page.jsp", SRC, &cfg).is_empty());
    }

    #[test]
    fn ignored_and_generated_files_are_skipped() {
        let mut cfg = config("java", &[(Target::Method, Convention::Camel)]);
        cfg.ignore = vec!["**/legacy/**".into()];
        assert!(violations("src/legacy/Order.java", SRC, &cfg).is_empty());
        assert!(violations("target/generated-sources/Order.java", SRC, &cfg).is_empty());
        let banner = format!("// Generated by wsdl2java\n{SRC}");
        assert!(violations("src/Order.java", &banner, &cfg).is_empty());
    }

    #[test]
    fn the_whole_standard_flags_each_kind_once() {
        let cfg = config("java", crate::java::JAVA.standard);
        let found = violations("src/Order.java", SRC, &cfg);
        let names: Vec<&str> = found.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, ["order_service", "item_count", "do_work", "the_input", "a_local"]);
        assert_eq!(found[0].suggested, "OrderService");
    }

    #[test]
    fn the_diagnostic_is_weak_and_carries_its_target_code() {
        let cfg = config("java", &[(Target::Method, Convention::Camel)]);
        let diags = diagnostics("src/Order.java", SRC, &cfg);
        assert_eq!(diags[0].severity, "weak");
        assert_eq!(diags[0].code, "naming-method");
        assert_eq!(diags[0].message, "Method `do_work` should be `doWork` (camelCase convention)");
        // The span is the identifier alone, so the squiggle sits under the name.
        assert_eq!(&SRC[diags[0].start..diags[0].end], "do_work");
    }

    #[test]
    fn the_caret_finds_the_violation_it_sits_in() {
        let cfg = config("java", crate::java::JAVA.standard);
        let at = SRC.find("do_work").expect("present");
        assert_eq!(violation_at("src/Order.java", SRC, &cfg, at).unwrap().name, "do_work");
        // Immediately after the last character still counts.
        let end = at + "do_work".len();
        assert_eq!(violation_at("src/Order.java", SRC, &cfg, end).unwrap().name, "do_work");
        // Somewhere with no declaration under it does not.
        assert!(violation_at("src/Order.java", SRC, &cfg, 0).is_none());
    }

    #[test]
    fn only_a_grammar_pack_marks_a_violation_file_local() {
        let cfg = config("java", crate::java::JAVA.standard);
        let found = violations("src/Order.java", SRC, &cfg);
        let local: Vec<&str> =
            found.iter().filter(|v| v.file_local).map(|v| v.name.as_str()).collect();
        assert_eq!(local, ["the_input", "a_local"]);
    }

    // ── the server-backed route ─────────────────────────────────────────────────

    fn ts_symbol(kind: &str, name: &str, source: &str) -> LspSymbol {
        let start = source.find(name).unwrap_or(0);
        LspSymbol {
            name: name.to_string(),
            kind: kind.to_string(),
            detail: None,
            start,
            end: start + name.len(),
            name_start: start,
            name_end: start + name.len(),
            line: 1,
            col: 1,
            file: "a.ts".to_string(),
            deprecated: false,
            children: Vec::new(),
        }
    }

    #[test]
    fn a_server_backed_file_is_checked_from_its_outline() {
        let source = "class order_service {}";
        let cfg = config("typescript", &[(Target::Type, Convention::Pascal)]);
        assert!(needs_symbols("src/app.ts", source, &cfg));
        // Its grammar route is empty — there is no grammar for it.
        assert!(violations("src/app.ts", source, &cfg).is_empty());

        let outline = [ts_symbol("class", "order_service", source)];
        let found = violations_from_symbols("src/app.ts", &outline, source, &cfg);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].suggested, "OrderService");
        // Nothing from an outline is ever renamed without a preview.
        assert!(!found[0].file_local);
    }

    #[test]
    fn a_grammar_file_never_asks_for_an_outline() {
        let cfg = config("java", crate::java::JAVA.standard);
        assert!(!needs_symbols("src/Order.java", SRC, &cfg));
        // And handing one to the symbols route is refused rather than mixed in.
        let outline = [ts_symbol("class", "order_service", SRC)];
        assert!(violations_from_symbols("src/Order.java", &outline, SRC, &cfg).is_empty());
    }

    #[test]
    fn a_generated_server_backed_file_is_skipped_before_any_outline_is_fetched() {
        let cfg = config("typescript", &[(Target::Type, Convention::Pascal)]);
        let source = "// Auto-generated by protoc\nclass order_service {}";
        assert!(!needs_symbols("src/app.ts", source, &cfg));
        assert!(!needs_symbols("node_modules/x/app.ts", "class order_service {}", &cfg));
    }
}

#[cfg(test)]
mod reserved_word_tests {
    use crate::config::{LanguageRules, NamingConfig};
    use crate::convention::Convention;
    use crate::target::Target;
    use std::collections::BTreeMap;

    fn config(pairs: &[(Target, Convention)]) -> NamingConfig {
        NamingConfig {
            enabled: true,
            ignore: Vec::new(),
            rules: BTreeMap::from([(
                "java".to_string(),
                LanguageRules::from_pairs(pairs.iter().copied()),
            )]),
            overrides: Vec::new(),
        }
    }

    /// `CONST` under `snake_case` renders as `const`, a Java reserved word. Apache Commons declares
    /// `ObjectUtils.CONST`, and offering the rename produced a file that no longer parses.
    #[test]
    fn a_suggestion_that_is_a_reserved_word_is_not_offered() {
        let cfg = config(&[(Target::Method, Convention::LowerSnake)]);
        let src = "class A { static boolean CONST(boolean v) { return v; } }";
        let found = super::violations("A.java", src, &cfg);
        assert!(found.iter().all(|v| v.name != "CONST"), "{found:?}");
    }

    /// A neighbouring name that renders to something legal is still offered — the guard is about
    /// the RESULT, not about switching the rule off.
    #[test]
    fn an_ordinary_name_is_still_offered() {
        let cfg = config(&[(Target::Method, Convention::LowerSnake)]);
        let src = "class A { static boolean CONSTANT(boolean v) { return v; } }";
        let found = super::violations("A.java", src, &cfg);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].suggested, "constant");
    }
}
