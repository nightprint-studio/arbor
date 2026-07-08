//! `switch` diagnostics — two things Java's `switch` (unlike a Rust `match`) is strict about:
//!
//!   * **selector type** — `switch` accepts `int`/`short`/`byte`/`char` (and their boxes), `String`
//!     and `enum` only. A `long`, `float`, `double` or `boolean` selector is a compile error.
//!   * **switch *expression* exhaustiveness of value** — when a `switch` is used as a value, every
//!     arm must produce one: an arrow `case X -> expr` does; a block arm `case X -> { … }` (or a
//!     colon group) must `yield` (or `throw`) on every path.
//!
//! Conservative:
//!   * the selector check flags only the four primitive types that are *never* legal
//!     (`long`/`float`/`double`/`boolean`) — an object selector is left alone (a project `enum`
//!     carries default flags, so we can't safely tell it from a non-switchable class);
//!   * the value check runs only when the `switch` is unmistakably in a value position (a
//!     variable initializer, a `return`, an assignment) and, like the missing-return check, assumes
//!     un-modelled control flow (loops, nested switches, `try`) yields — so it never false-flags.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

// ── selector type (pure AST) ─────────────────────────────────────────────────
//
// `switch` accepts int/short/byte/char (+ boxes), `String` and `enum`. Only the four *primitive*
// types that are never legal are flagged — and those are purely syntactic (a `long`/`double`/… local,
// parameter or field, or a `long`/floating/boolean literal), so no resolver / inference is needed
// (Bennu's inference doesn't even model primitives). An object selector is left alone (a project
// `enum` would be indistinguishable from a non-switchable class).

const FORBIDDEN_SELECTOR: [&str; 4] = ["long", "float", "double", "boolean"];

/// Parse `source` and flag `switch` selectors of a primitive type `switch` doesn't accept.
pub fn switch_selector_errors(source: &str) -> Vec<Diagnostic> {
    with_parse(source, |root| switch_selector_errors_in(root, source))
}

/// Tree-driven core.
pub fn switch_selector_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    switch_selector_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn switch_selector_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "switch_expression" {
            if let Some(cond) = n.child_by_field_name("condition") {
                if let Some(ty) = selector_primitive(cond, bytes) {
                    if FORBIDDEN_SELECTOR.contains(&ty) {
                        out.push(err(
                            format!(
                                "`switch` on `{ty}` is not permitted (only int-family, `String` and `enum`)"
                            ),
                            cond,
                        ));
                    }
                }
            }
        }
    }
    out
}

/// If the selector is *syntactically* a forbidden primitive — a literal, or an identifier whose
/// declared type (local / parameter / field, found by a scope walk) is one — return that primitive.
fn selector_primitive(cond: Node, bytes: &[u8]) -> Option<&'static str> {
    // Unwrap `(expr)`.
    let inner = if cond.kind() == "parenthesized_expression" {
        cond.named_child(0)?
    } else {
        cond
    };
    match inner.kind() {
        // Literals.
        "true" | "false" => Some("boolean"),
        "decimal_floating_point_literal" | "hex_floating_point_literal" => {
            let t = inner.utf8_text(bytes).unwrap_or("");
            if t.ends_with('f') || t.ends_with('F') {
                Some("float")
            } else {
                Some("double")
            }
        }
        "decimal_integer_literal" | "hex_integer_literal" => {
            let t = inner.utf8_text(bytes).unwrap_or("");
            (t.ends_with('l') || t.ends_with('L')).then_some("long")
        }
        // A bare name: resolve its declared type via a scope walk.
        "identifier" => {
            let name = inner.utf8_text(bytes).ok()?;
            let ty = declared_type_text(inner, name, bytes)?;
            FORBIDDEN_SELECTOR.iter().copied().find(|&p| p == ty)
        }
        _ => None,
    }
}

/// The declared type text of `name` as visible at `use_node`: a method parameter, or a local
/// variable declared before the use, or a field of the enclosing type. Walks ancestor scopes — a
/// small, syntactic subset of the inference engine's local resolution (enough for primitive
/// selectors).
fn declared_type_text(use_node: Node, name: &str, bytes: &[u8]) -> Option<String> {
    let use_start = use_node.start_byte();
    let mut scope = use_node.parent();
    while let Some(s) = scope {
        // parameters
        if let Some(params) = s.child_by_field_name("parameters") {
            let mut pw = params.walk();
            for p in params.named_children(&mut pw) {
                if matches!(p.kind(), "formal_parameter" | "spread_parameter") {
                    if p.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name) {
                        return p.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()).map(str::to_string);
                    }
                }
            }
        }
        // locals + fields declared directly in this scope, before the use.
        let mut cw = s.walk();
        for c in s.named_children(&mut cw) {
            if c.start_byte() >= use_start {
                break;
            }
            if matches!(c.kind(), "local_variable_declaration" | "field_declaration") {
                if let Some(t) = declarator_type(c, name, bytes) {
                    return Some(t);
                }
            }
        }
        scope = s.parent();
    }
    None
}

/// The declared type text of a `local_variable_declaration` / `field_declaration` if it declares
/// `name`.
fn declarator_type(decl: Node, name: &str, bytes: &[u8]) -> Option<String> {
    let ty = decl.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok())?;
    let mut dw = decl.walk();
    for d in decl.named_children(&mut dw) {
        if d.kind() == "variable_declarator"
            && d.child_by_field_name("name").and_then(|n| n.utf8_text(bytes).ok()) == Some(name)
        {
            return Some(ty.to_string());
        }
    }
    None
}

// ── switch-expression value production (pure AST) ─────────────────────────────

/// Parse `source` and flag switch-expression arms that don't produce a value.
pub fn switch_yield_errors(source: &str) -> Vec<Diagnostic> {
    with_parse(source, |root| switch_yield_errors_in(root, source))
}

/// Tree-driven core.
pub fn switch_yield_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    switch_yield_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn switch_yield_errors_nodes(nodes: &[Node], _source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "switch_expression" && is_value_context(n) && !n.has_error() {
            check_arms(n, &mut out);
        }
    }
    out
}

fn check_arms(switch: Node, out: &mut Vec<Diagnostic>) {
    let Some(body) = switch.child_by_field_name("body") else { return };
    let mut c = body.walk();
    for arm in body.named_children(&mut c) {
        match arm.kind() {
            // `case X -> …`
            "switch_rule" => {
                // The rule's value is its last child (an expression_statement / block / throw).
                let Some(val) = last_named(arm) else { continue };
                let ok = match val.kind() {
                    "block" => block_definitely_yields(val),
                    "throw_statement" | "expression_statement" => true,
                    _ => true, // an expression body produces a value
                };
                if !ok {
                    out.push(err(
                        "A `switch` expression arm must produce a value (`yield` or `throw`)".to_string(),
                        arm,
                    ));
                }
            }
            // `case X: …; yield …;`
            "switch_block_statement_group" => {
                if !group_definitely_yields(arm) {
                    out.push(err(
                        "A `switch` expression branch must `yield` a value on every path".to_string(),
                        arm,
                    ));
                }
            }
            _ => {}
        }
    }
}

/// Whether `switch` sits where a value is required (its result is used). Kept to unmistakable
/// expression positions so a plain statement `switch` is never mistaken for one.
pub(crate) fn is_value_context(switch: Node) -> bool {
    let Some(p) = switch.parent() else { return false };
    match p.kind() {
        "variable_declarator" => p.child_by_field_name("value") == Some(switch),
        "return_statement" => true,
        "assignment_expression" => p.child_by_field_name("right") == Some(switch),
        "argument_list" | "binary_expression" | "ternary_expression" | "array_initializer" => true,
        _ => false,
    }
}

/// A block guarantees a `yield`/`throw` on every path (mirrors the missing-return reachability, with
/// `yield` as the producing statement). Conservative: un-modelled control flow is assumed to yield.
fn block_definitely_yields(block: Node) -> bool {
    let mut last: Option<Node> = None;
    let mut c = block.walk();
    for ch in block.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment") {
            continue;
        }
        last = Some(ch);
    }
    last.map(stmt_definitely_yields).unwrap_or(false)
}

fn group_definitely_yields(group: Node) -> bool {
    let mut last: Option<Node> = None;
    let mut c = group.walk();
    for ch in group.named_children(&mut c) {
        if matches!(ch.kind(), "line_comment" | "block_comment" | "switch_label") {
            continue;
        }
        last = Some(ch);
    }
    last.map(stmt_definitely_yields).unwrap_or(false)
}

fn stmt_definitely_yields(stmt: Node) -> bool {
    match stmt.kind() {
        "yield_statement" | "throw_statement" | "return_statement" => true,
        "expression_statement" => false,
        "block" => block_definitely_yields(stmt),
        "if_statement" => {
            match (stmt.child_by_field_name("consequence"), stmt.child_by_field_name("alternative")) {
                (Some(cons), Some(alt)) => stmt_definitely_yields(cons) && stmt_definitely_yields(alt),
                _ => false,
            }
        }
        // Not fully modelled → assume it yields (never false-flag).
        "for_statement" | "enhanced_for_statement" | "while_statement" | "do_statement"
        | "switch_expression" | "try_statement" | "try_with_resources_statement"
        | "labeled_statement" | "synchronized_statement" => true,
        _ => false,
    }
}

fn last_named(n: Node) -> Option<Node> {
    let mut c = n.walk();
    let mut last = None;
    for ch in n.named_children(&mut c) {
        if !matches!(ch.kind(), "line_comment" | "block_comment" | "switch_label") {
            last = Some(ch);
        }
    }
    last
}

fn err(message: String, node: Node) -> Diagnostic {
    Diagnostic { message, severity: "error".to_string(), code: String::new(), start: node.start_byte(), end: node.end_byte() }
}

fn with_parse(source: &str, f: impl FnOnce(Node) -> Vec<Diagnostic>) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    match parser.parse(source, None) {
        Some(tree) => f(tree.root_node()),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(body: &str) -> Vec<String> {
        let src = format!("class C {{ void m() {{ {body} }} }}");
        switch_selector_errors(&src).into_iter().map(|d| d.message).collect()
    }
    fn yld(body: &str) -> Vec<String> {
        let src = format!("class C {{ int m() {{ {body} }} }}");
        switch_yield_errors(&src).into_iter().map(|d| d.message).collect()
    }

    // ── selector type ──────────────────────────────────────────────────────────

    #[test]
    fn long_selector_is_flagged() {
        let d = sel("long l = 0; switch (l) { default: break; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("long"), "{d:?}");
    }

    #[test]
    fn boolean_and_double_selectors_are_flagged() {
        assert!(sel("boolean b = true; switch (b) { default: break; }")[0].contains("boolean"));
        assert!(sel("double d = 0; switch (d) { default: break; }")[0].contains("double"));
    }

    #[test]
    fn int_selector_is_ok() {
        assert!(sel("int x = 0; switch (x) { default: break; }").is_empty());
    }

    #[test]
    fn unknown_selector_is_not_flagged() {
        // A selector whose type can't be inferred → silent.
        assert!(sel("switch (compute()) { default: break; }").is_empty());
    }

    // ── switch-expression value production ──────────────────────────────────────

    #[test]
    fn arrow_expression_arms_are_ok() {
        assert!(yld("int y = switch (0) { case 0 -> 1; default -> 2; }; return y;").is_empty());
    }

    #[test]
    fn block_arm_without_yield_is_flagged() {
        let d = yld("int y = switch (0) { case 0 -> { int z = 1; } default -> 2; }; return y;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("produce a value"), "{d:?}");
    }

    #[test]
    fn block_arm_with_yield_is_ok() {
        assert!(yld("int y = switch (0) { case 0 -> { yield 1; } default -> 2; }; return y;").is_empty());
    }

    #[test]
    fn block_arm_with_throw_is_ok() {
        assert!(yld(
            "int y = switch (0) { case 0 -> { throw new RuntimeException(); } default -> 2; }; return y;"
        )
        .is_empty());
    }

    #[test]
    fn statement_switch_is_not_checked() {
        // A plain statement switch (no value) → arms needn't yield.
        assert!(yld("switch (0) { case 0 -> { int z = 1; } default -> {} } return 0;").is_empty());
    }

    #[test]
    fn colon_group_without_yield_is_flagged() {
        let d = yld("int y = switch (0) { case 0: System.out.println(); default: yield 2; }; return y;");
        assert!(d.iter().any(|m| m.contains("yield")), "{d:?}");
    }
}
