//! Syntax-error diagnostics from the tree-sitter `ERROR` / `MISSING` nodes.
//!
//! tree-sitter is error-tolerant: a malformed source still parses, marking the broken region with
//! an `ERROR` node and inserting zero-width `MISSING` nodes where a required token (a `;`, a `)`, a
//! brace) is absent. We surface both as `error` diagnostics — the "red squiggle before you compile"
//! for the syntax class, with no Maven/javac needed.
//!
//! Two noise guards: a multi-line `ERROR` span is clamped to the end of its first line (a
//! whole-block red squiggle is useless), and clean subtrees are pruned from the walk via
//! `has_error()`.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Every syntax error (tree-sitter `ERROR` / `MISSING`) in `root`, as `error` diagnostics with
/// byte-offset spans. Empty for a well-formed tree.
pub fn syntax_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    // A fully-valid parse has no error anywhere — the common case, checked once.
    if !root.has_error() {
        return Vec::new();
    }
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.is_error() {
            out.push(error_diag(n, bytes));
            continue; // one squiggle per error subtree — don't descend into the wreckage
        }
        if n.is_missing() {
            out.push(missing_diag(n));
            continue;
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            // Descend only where an error actually lives (prune clean subtrees).
            if ch.has_error() {
                stack.push(ch);
            }
        }
    }
    out
}

/// A diagnostic for an `ERROR` node, its span clamped to the first line so a multi-line error
/// doesn't paint a whole block red.
fn error_diag(n: Node, bytes: &[u8]) -> Diagnostic {
    let start = n.start_byte();
    let mut end = n.end_byte().min(bytes.len());
    if start < end {
        if let Some(nl) = bytes[start..end].iter().position(|&b| b == b'\n') {
            end = start + nl;
        }
    }
    if end <= start {
        end = start; // zero-width → the FE widens it to one glyph
    }
    Diagnostic {
        message: "Syntax error".to_string(),
        severity: crate::check_id::CheckId::SyntaxError.severity().to_string(),
        code: crate::check_id::CheckId::SyntaxError.code().to_string(),
        start,
        end,
    }
}

/// A diagnostic for a zero-width `MISSING` node — the parser expected a token here. `kind()` is the
/// expected token (`;`, `)`, `}`), so the message names it.
fn missing_diag(n: Node) -> Diagnostic {
    let start = n.start_byte();
    let kind = n.kind();
    Diagnostic {
        message: format!("Missing `{kind}`"),
        severity: crate::check_id::CheckId::MissingToken.severity().to_string(),
        code: crate::check_id::CheckId::MissingToken.code().to_string(),
        start,
        end: start, // zero-width — the FE widens the marker to one glyph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse(src: &str) -> tree_sitter::Tree {
        let mut p = Parser::new();
        p.set_language(&tree_sitter_java::LANGUAGE.into()).unwrap();
        p.parse(src, None).unwrap()
    }

    fn errors(src: &str) -> Vec<Diagnostic> {
        let tree = parse(src);
        syntax_errors(tree.root_node(), src)
    }

    /// Through the crate's real entry point, which is where a parse that failed for OUR reasons is
    /// retried — see `bennu_java::grammar`.
    fn errors_via_engine(src: &str) -> Vec<Diagnostic> {
        let tree = bennu_java::prelude::parse_java(src).expect("a parse");
        syntax_errors(tree.root_node(), src)
    }

    /// `Object @Nullable ... args` is legal Java the grammar cannot parse. Reporting it calls the
    /// user's code broken when ours is, and a syntax error suppresses every semantic check in the
    /// file — so one parameter blanked eight whole sources on Guava.
    #[test]
    fn a_type_use_annotation_on_varargs_is_not_reported() {
        let src = "class A {\n  static String g(String t, Object @Nullable ... a) { return t; }\n}\n";
        assert!(errors_via_engine(src).is_empty(), "{:?}", errors_via_engine(src));
    }

    /// The guard is narrow: a real syntax error in the same file is still reported.
    #[test]
    fn an_ordinary_syntax_error_is_still_reported() {
        let src = "class A {\n  static String g(String t, Object @Nullable ... a) { return t }\n}\n";
        assert!(!errors_via_engine(src).is_empty());
    }

    #[test]
    fn well_formed_source_has_no_errors() {
        let src = "package com.acme;\npublic class Foo {\n  int x = 3;\n  void run() { x++; }\n}\n";
        assert!(errors(src).is_empty());
    }

    #[test]
    fn broken_statement_is_flagged() {
        let src = "class Foo { void run() { int x = ; } }";
        let e = errors(src);
        assert!(!e.is_empty(), "a broken statement should produce at least one error");
        assert!(e.iter().all(|d| d.severity == "error"));
    }

    #[test]
    fn missing_brace_is_flagged() {
        // Unclosed class body — the parser marks the missing `}`.
        let src = "class Foo { void run() { ";
        let e = errors(src);
        assert!(!e.is_empty());
    }

    #[test]
    fn error_span_is_clamped_to_first_line() {
        let src = "class Foo {\n  void run() { int x = ;\n    int y = 2;\n  }\n}\n";
        for d in errors(src) {
            let span = &src.as_bytes()[d.start..d.end.min(src.len())];
            assert!(!span.contains(&b'\n'), "error span must not cross a newline");
        }
    }
}

#[cfg(test)]
mod parse_gate_tests {
    use crate::check::{check_file_resolved, FileContext};
    use bennu_java::prelude::{ClassMembers, Import, TypeResolver};
    use std::collections::HashMap;

    struct Empty;
    impl TypeResolver for Empty {
        fn members_of(&self, _b: &str) -> Option<std::sync::Arc<ClassMembers>> {
            None
        }
        fn resolve_simple_name(&self, _n: &str, _i: &[Import]) -> Option<String> {
            None
        }
        fn is_project_type(&self, _b: &str) -> bool {
            false
        }
    }

    fn run(src: &str) -> Vec<String> {
        let ctx = FileContext::default();
        check_file_resolved(src, &ctx, &Empty, true).into_iter().map(|d| d.code).collect()
    }

    /// A file that does not parse reports its syntax error and stops. Reading a broken tree as if it
    /// meant something is how one fuzzer-test file produced 199 undefined symbols named after the
    /// CONTENTS of a string literal.
    #[test]
    fn a_file_that_does_not_parse_reports_only_syntax() {
        let codes = run("package p;\nclass A { void m( { int x = ; } }\n");
        assert!(codes.contains(&"syntax-error".to_string()), "{codes:?}");
        // Syntactic findings are fine — they are about the text. What must not appear is anything
        // that read the tree as if it meant something.
        assert!(
            codes.iter().all(|c| matches!(c.as_str(), "syntax-error" | "missing-token")),
            "a broken parse must not produce semantic diagnostics: {codes:?}"
        );
    }

    /// A file that parses is checked normally — the gate must not switch the checks off wholesale.
    #[test]
    fn a_file_that_parses_is_still_checked() {
        let codes = run("package p;\nclass A { void m() { int x = 1; } }\n");
        assert!(!codes.contains(&"syntax-error".to_string()), "{codes:?}");
    }
    #[allow(dead_code)]
    fn _unused(_: HashMap<String, ClassMembers>) {}
}
