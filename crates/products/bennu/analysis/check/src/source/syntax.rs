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
            out.push(missing_diag(n, bytes));
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
        message: describe_error(n, bytes),
        severity: crate::engine::check_id::CheckId::SyntaxError.severity().to_string(),
        code: crate::engine::check_id::CheckId::SyntaxError.code().to_string(),
        start,
        end,
    }
}

/// What a tree-sitter `ERROR` node most likely means, said the way javac says it.
///
/// tree-sitter knows only that tokens did not fit; the message has to be recovered from WHAT the
/// error starts with and WHERE it sits. The guesses are deliberately the plain ones — javac's own
/// (`illegal start of expression`, `class, interface, enum, or record expected`, `reached end of
/// file while parsing`) — and the last resort quotes the offending token rather than saying nothing,
/// which is what "Syntax error" on its own amounted to.
fn describe_error(n: Node, bytes: &[u8]) -> String {
    let text = n.utf8_text(bytes).unwrap_or("");
    let trimmed = text.trim_start();
    let first: String = trimmed
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
        .collect();
    let first_char = trimmed.chars().next();

    // Nothing after the error but whitespace and comments: the file simply stops.
    if bytes.get(n.end_byte()..).is_some_and(|rest| rest.iter().all(u8::is_ascii_whitespace)) && trimmed.is_empty() {
        return "Reached the end of the file while parsing".to_string();
    }
    match first.as_str() {
        "else" => return "`else` without `if`".to_string(),
        "catch" => return "`catch` without `try`".to_string(),
        "finally" => return "`finally` without `try`".to_string(),
        "try" => return "`try` without `catch`, `finally` or resource declarations".to_string(),
        "case" | "default" if !inside(n, &["switch_block", "switch_body"]) => {
            return format!("Orphaned `{first}` — it belongs inside a `switch`");
        }
        _ => {}
    }
    let parent = n.parent().map(|p| p.kind()).unwrap_or("");
    let at_top_level = parent == "program";
    if at_top_level {
        return "A class, interface, enum or record declaration is expected here".to_string();
    }
    // An operator, a closer or a separator where an expression has to start.
    if matches!(first_char, Some('=' | '+' | '*' | '/' | '%' | '<' | '>' | '|' | '&' | '?' | ':' | ')' | ']' | ',' | '.')) {
        return format!("Illegal start of expression at `{}`", snippet(trimmed));
    }
    if parent == "class_body" || parent == "interface_body" || parent == "enum_body_declarations" {
        return match first.as_str() {
            "" => format!("Illegal start of type at `{}`", snippet(trimmed)),
            _ => "A member declaration is expected here — a statement cannot stand directly in a class body".to_string(),
        };
    }
    if matches!(first_char, Some('}' | '{')) {
        return format!("Unexpected `{}` — braces do not match", first_char.unwrap_or('}'));
    }
    if trimmed.is_empty() {
        return "Syntax error: something is missing here".to_string();
    }
    format!("Syntax error near `{}`", snippet(trimmed))
}

/// The first few characters of an offending region, on one line, for a message to quote.
fn snippet(text: &str) -> String {
    let line = text.lines().next().unwrap_or("");
    let short: String = line.chars().take(24).collect();
    if line.chars().count() > 24 { format!("{short}…") } else { short }
}

/// Whether `n` sits inside a node of one of `kinds`.
fn inside(n: Node, kinds: &[&str]) -> bool {
    let mut cur = n.parent();
    while let Some(p) = cur {
        if kinds.contains(&p.kind()) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// What a `MISSING` token's absence is called: javac's own `';' expected`, with the grammar's
/// internal names (`identifier`, `type_identifier`) turned into `<identifier>`.
fn expected_token(kind: &str) -> String {
    match kind {
        "identifier" | "type_identifier" => "`<identifier>` expected".to_string(),
        other => format!("`{other}` expected"),
    }
}

/// A diagnostic for a zero-width `MISSING` node — the parser expected a token here. `kind()` is the
/// expected token (`;`, `)`, `}`), so the message names it.
fn missing_diag(n: Node, bytes: &[u8]) -> Diagnostic {
    let start = n.start_byte();
    let kind = n.kind();
    // A closer missing with nothing after it: javac's "reached end of file while parsing".
    let at_end = bytes.get(start..).is_some_and(|rest| rest.iter().all(u8::is_ascii_whitespace));
    let message = if at_end && matches!(kind, "}" | ")" | "]") {
        format!("Reached the end of the file while parsing — {}", expected_token(kind))
    } else {
        expected_token(kind)
    };
    Diagnostic {
        message,
        severity: crate::engine::check_id::CheckId::MissingToken.severity().to_string(),
        code: crate::engine::check_id::CheckId::MissingToken.code().to_string(),
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
    use crate::engine::check::{check_file_resolved, FileContext};
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

#[cfg(test)]
mod message_tests {
    use super::*;

    fn messages(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("a parse");
        syntax_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_missing_semicolon_is_named_like_javac() {
        assert_eq!(messages("class A { void m() { int x = 1 } }"), ["`;` expected"]);
    }

    #[test]
    fn a_dangling_else_catch_and_try_are_named() {
        assert_eq!(messages("class A { void m() { else { } } }"), ["`else` without `if`"]);
        assert!(messages("class A { void m() { try { } } }")[0].starts_with("`try` without `catch`"));
    }

    #[test]
    fn a_missing_operand_is_an_illegal_start_of_expression() {
        assert!(messages("class A { int x = 1 + ; }")[0].starts_with("Illegal start of expression"));
    }

    #[test]
    fn a_stray_closing_brace_says_a_declaration_is_expected() {
        assert!(messages("class A { } }")[0].starts_with("A class, interface, enum or record"));
    }

    #[test]
    fn a_statement_in_a_class_body_says_so() {
        assert!(messages("class A { foo(); }")[0].contains("class body"));
    }

    #[test]
    fn a_file_that_stops_early_says_it_ended() {
        assert!(messages("class A { void m() { ").iter().all(|m| m.starts_with("Reached the end of the file")));
    }

    #[test]
    fn no_message_is_the_bare_old_one() {
        for src in ["class A { void m() { x = ; ) } }", "class A { void m() { foo(1, ; } }", "class A { void m() { case 1: break; } }"] {
            assert!(messages(src).iter().all(|m| m != "Syntax error"), "{src}: {:?}", messages(src));
        }
    }
}
