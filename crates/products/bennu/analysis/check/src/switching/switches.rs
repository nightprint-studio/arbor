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
use tree_sitter::Node;

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
    switch_selector_errors_nodes(&crate::engine::check::collect_nodes(root), source)
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
                        out.push(crate::engine::check_id::CheckId::IllegalSwitchSelector.at(
                            cond,
                            format!(
                                "`switch` on `{ty}` is not permitted (only int-family, `String` and `enum`)"
                            ),
                        ));
                    }
                }
                if let (Some(ty), Some(body)) = (integral_selector(cond, bytes), n.child_by_field_name("body")) {
                    check_integral_labels(body, ty, bytes, &mut out);
                }
            }
        }
    }
    out
}

/// The integral primitive a selector is declared as (`int`, `short`, `byte`, `char`), read the same
/// syntactic way as the forbidden ones.
fn integral_selector(cond: Node, bytes: &[u8]) -> Option<&'static str> {
    let inner = if cond.kind() == "parenthesized_expression" { cond.named_child(0)? } else { cond };
    if inner.kind() != "identifier" {
        return None;
    }
    let name = inner.utf8_text(bytes).ok()?;
    let ty = crate::support::scopes::declared_type_text(inner, name, bytes, true)?;
    ["int", "short", "byte", "char"].into_iter().find(|p| *p == ty)
}

/// Labels an integral selector can never take: a string, or an integer constant outside its range
/// (`case 200` on a `byte`). Only decimal literals are read; anything else is left alone.
fn check_integral_labels(body: Node, ty: &str, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let (min, max): (i64, i64) = match ty {
        "byte" => (-128, 127),
        "short" => (-32768, 32767),
        "char" => (0, 65535),
        _ => (i64::from(i32::MIN), i64::from(i32::MAX)),
    };
    for label in crate::support::switch_label::labels_of(body) {
        if crate::support::switch_label::label_is_pattern(label) {
            continue;
        }
        let mut lc = label.walk();
        for cst in label.named_children(&mut lc) {
            let Ok(text) = cst.utf8_text(bytes) else { continue };
            let message = if matches!(cst.kind(), "string_literal" | "text_block") {
                format!("`case {}` is a string, which can't match an `{ty}` selector", crate::support::text::short(text))
            } else if let Some(value) = decimal_value(cst, bytes).filter(|v| *v < min || *v > max) {
                format!("`case {value}` does not fit in the `{ty}` selector")
            } else {
                continue;
            };
            out.push(crate::engine::check_id::CheckId::IncompatibleCaseLabel.at(cst, message));
        }
    }
}

/// The value of a plain decimal literal, optionally negated.
fn decimal_value(n: Node, bytes: &[u8]) -> Option<i64> {
    let (sign, lit) = if n.kind() == "unary_expression" {
        let op = n.child_by_field_name("operator")?.utf8_text(bytes).ok()?;
        (if op == "-" { -1 } else { 1 }, n.child_by_field_name("operand")?)
    } else {
        (1, n)
    };
    if lit.kind() != "decimal_integer_literal" {
        return None;
    }
    let digits: String = lit.utf8_text(bytes).ok()?.chars().filter(|c| *c != '_').collect();
    digits.parse::<i64>().ok().map(|v| sign * v)
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
            let ty = crate::support::scopes::declared_type_text(inner, name, bytes, true)?;
            FORBIDDEN_SELECTOR.iter().copied().find(|&p| p == ty)
        }
        _ => None,
    }
}

// ── switch-expression value production (pure AST) ─────────────────────────────

/// Parse `source` and flag switch-expression arms that don't produce a value.
pub fn switch_yield_errors(source: &str) -> Vec<Diagnostic> {
    with_parse(source, |root| switch_yield_errors_in(root, source))
}

/// Tree-driven core.
pub fn switch_yield_errors_in(root: Node, source: &str) -> Vec<Diagnostic> {
    switch_yield_errors_nodes(&crate::engine::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn switch_yield_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "switch_expression" && is_value_context(n) && !n.has_error() {
            check_arms(n, bytes, &mut out);
        }
    }
    out
}

/// The arms of a switch EXPRESSION, judged with the same reachability rules as a method body
/// ([`crate::flow::returns::can_complete_normally`]):
///
/// * an arrow arm's block must not complete normally — it `yield`s or throws;
/// * a colon group may fall through into the next one, so only the LAST group is held to that;
/// * the switch must have an arm at all, and at least one arm that produces a value.
fn check_arms(switch: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = switch.child_by_field_name("body") else { return };
    let mut c = body.walk();
    let arms: Vec<Node> = body
        .named_children(&mut c)
        .filter(|a| matches!(a.kind(), "switch_rule" | "switch_block_statement_group"))
        .collect();
    let Some(condition) = switch.child_by_field_name("condition") else { return };
    if arms.is_empty() {
        out.push(err("A `switch` expression must have at least one case".to_string(), condition));
        return;
    }
    let mut produces_value = false;
    for (i, arm) in arms.iter().enumerate() {
        match arm.kind() {
            // `case X -> …`
            "switch_rule" => {
                let Some(val) = last_named(*arm) else { continue };
                match val.kind() {
                    "expression_statement" => produces_value = true,
                    "block" => {
                        // A block may yield somewhere inside; only one that certainly cannot is a
                        // "no result" switch, and that is not worth the analysis.
                        produces_value = true;
                        if crate::flow::returns::can_complete_normally(val, bytes) {
                            out.push(err(
                                "A `switch` expression arm must produce a value (`yield` or `throw`)".to_string(),
                                *arm,
                            ));
                        }
                    }
                    _ => {}
                }
            }
            // `case X: …; yield …;`
            "switch_block_statement_group" => {
                produces_value = true;
                if i + 1 == arms.len() && group_can_complete_normally(*arm, bytes) {
                    out.push(err(
                        "A `switch` expression branch must `yield` a value on every path".to_string(),
                        *arm,
                    ));
                }
            }
            _ => {}
        }
    }
    if !produces_value {
        out.push(err(
            "A `switch` expression needs at least one arm that produces a value".to_string(),
            condition,
        ));
    }
}

/// Whether control can run off the end of a colon group — no statements at all, or statements that
/// all complete normally.
fn group_can_complete_normally(group: Node, bytes: &[u8]) -> bool {
    let mut c = group.walk();
    let statements: Vec<Node> = group
        .named_children(&mut c)
        .filter(|s| !matches!(s.kind(), "line_comment" | "block_comment" | "switch_label"))
        .collect();
    statements.iter().all(|s| crate::flow::returns::can_complete_normally(*s, bytes))
}

/// Whether `switch` stands as a STATEMENT rather than producing a value.
///
/// The grammar has one node for both, so the answer is in the parent: a statement list, the body of
/// a statement, a label. An arrow arm's `expression_statement` is the arm's value when its own switch
/// is an expression, and a statement when its own switch is one.
pub(crate) fn is_statement_position(switch: Node) -> bool {
    let Some(p) = switch.parent() else { return false };
    match p.kind() {
        "block" | "switch_block_statement_group" | "labeled_statement" | "if_statement" | "while_statement"
        | "for_statement" | "enhanced_for_statement" | "do_statement" => true,
        "expression_statement" => match p.parent() {
            Some(rule) if rule.kind() == "switch_rule" => rule
                .parent()
                .and_then(|block| block.parent())
                .is_some_and(is_statement_position),
            Some(_) => true,
            None => false,
        },
        _ => false,
    }
}

/// A switch EXPRESSION over constant labels (`int`, `char`, `String` literals) with no `default`.
///
/// Only an enum, a sealed hierarchy or a total pattern can make a switch exhaustive without one; a
/// label set made only of literals is none of those, whatever the selector's type.
pub fn constant_switch_exhaustiveness_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "switch_expression" || n.has_error() || is_statement_position(n) {
            continue;
        }
        let (Some(body), Some(cond)) = (n.child_by_field_name("body"), n.child_by_field_name("condition")) else {
            continue;
        };
        let labels = crate::support::switch_label::labels_of(body);
        if labels.is_empty() {
            continue; // the empty switch is its own error
        }
        let all_literal = labels.iter().all(|l| {
            if crate::support::switch_label::label_is_default(*l, bytes) || crate::support::switch_label::label_is_pattern(*l) {
                return false;
            }
            let mut lc = l.walk();
            let constants: Vec<Node> = l.named_children(&mut lc).collect();
            !constants.is_empty() && constants.iter().all(|c| is_literal_label(*c))
        });
        if all_literal {
            out.push(crate::engine::check_id::CheckId::NonExhaustiveEnumSwitch.at(
                cond,
                "A `switch` expression over constants is not exhaustive — add a `default`",
            ));
        }
    }
    out
}

fn is_literal_label(n: Node) -> bool {
    let inner = if n.kind() == "unary_expression" { n.child_by_field_name("operand") } else { Some(n) };
    inner.is_some_and(|i| {
        matches!(
            i.kind(),
            "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal" | "binary_integer_literal"
                | "character_literal" | "string_literal"
        )
    })
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
    Diagnostic { message, severity: crate::engine::check_id::CheckId::SwitchExpressionIncomplete.severity().to_string(), code: crate::engine::check_id::CheckId::SwitchExpressionIncomplete.code().to_string(), start: node.start_byte(), end: node.end_byte() }
}

fn with_parse(source: &str, f: impl FnOnce(Node) -> Vec<Diagnostic>) -> Vec<Diagnostic> {
    match bennu_java::prelude::parse_java(source) {
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
    fn last_colon_group_without_yield_is_flagged() {
        let d = yld("int y = switch (0) { case 0: yield 1; default: System.out.println(); }; return y;");
        assert!(d.iter().any(|m| m.contains("yield")), "{d:?}");
    }

    #[test]
    fn colon_groups_may_fall_through_to_a_yield() {
        assert!(yld("return switch (0) { case 1: case 2: System.gc(); default: yield 2; };").is_empty());
    }

    #[test]
    fn empty_and_valueless_switch_expressions_are_flagged() {
        assert!(yld("return switch (0) { };").iter().any(|m| m.contains("at least one case")));
        assert!(yld("return switch (0) { default -> throw new IllegalStateException(); };")
            .iter()
            .any(|m| m.contains("produces a value")));
    }

    #[test]
    fn constant_switch_expression_without_default_is_not_exhaustive() {
        let run = |body: &str| {
            let src = format!("class C {{ int m(int k) {{ {body} }} }}");
            let tree = bennu_java::prelude::parse_java(&src).unwrap();
            let nodes = crate::engine::check::collect_nodes(tree.root_node());
            constant_switch_exhaustiveness_nodes(&nodes, &src).len()
        };
        assert_eq!(run("return switch (k) { case 1 -> 1; case 2 -> 2; };"), 1);
        assert_eq!(run("return switch (k) { case 1 -> 1; default -> 2; };"), 0);
        // A name may be an enum constant: not ours to judge.
        assert_eq!(run("return switch (k) { case LOW -> 1; };"), 0);
        // A statement need not be exhaustive.
        assert_eq!(run("switch (k) { case 1 -> System.gc(); } return 0;"), 0);
    }

    #[test]
    fn integral_selector_label_types_and_ranges() {
        assert!(sel("byte b = 0; switch (b) { case 200: break; }").iter().any(|m| m.contains("does not fit")));
        assert!(sel("int i = 0; switch (i) { case \"a\": break; }").iter().any(|m| m.contains("string")));
        assert!(sel("byte b = 0; switch (b) { case -128: break; case 127: break; }").is_empty());
        assert!(sel("int i = 0; switch (i) { case 'a': break; }").is_empty());
    }
}
