//! Expression-level lints (pure-AST, all `warning` severity): small, structural checks that need no
//! type information and — following the crate's paramount rule — only fire on the syntactically
//! unambiguous case, never on anything that *might* be legitimate.
//!
//! Four independent checks, each conservative by construction:
//!   1. **Self-assignment** — `x = x` / `this.x = this.x`: LHS and RHS are byte-for-byte the same
//!      simple name (or `this.<ident>`). A no-op, and almost always a typo for a different RHS.
//!   2. **Constant division / modulo by zero** — `a / 0`, `a % 0L`: the RHS is *literally* an
//!      integer-zero literal. We never evaluate constant expressions, and float `0.0` is legal
//!      (Infinity/NaN), so both are left alone.
//!   3. **Empty statement** — a stray `;` that stands alone as a block-level statement. In this
//!      grammar an empty statement is an anonymous `;` token (not a named node), so we detect it as a
//!      `;` child *of a `block`* — which by construction excludes `for(;;)` clauses (children of the
//!      `for_statement`) and `while(cond);` bodies (a `;` child of the loop, not a block).
//!   4. **Empty `if` body** — `if (cond);`: the `;` IS the guarded statement, so whatever follows
//!      runs unconditionally. Unlike a stray `;` in a block (check 3) this one changes what the
//!      program does, which is why it is its own kind and not that one.
//!   5. **String `==` / `!=`** — one operand is syntactically a `string_literal`. Reference equality
//!      on a String literal is virtually always a bug; this is the subset provable without type info.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Slice-driven entry: iterate the shared pre-collected node list (one traversal across all pure-AST
/// checks) and emit every expression-level lint. Signature mirrors the other `*_nodes` checks.
pub fn expr_lint_warnings_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "assignment_expression" => {
                if let Some(d) = self_assignment(n, bytes) {
                    out.push(d);
                }
            }
            "binary_expression" => {
                if let Some(d) = div_mod_by_zero(n, bytes) {
                    out.push(d);
                }
                if let Some(d) = string_reference_equality(n, bytes) {
                    out.push(d);
                }
            }
            "block" => empty_statements_in_block(n, &mut out),
            "if_statement" => {
                if let Some(d) = empty_if_body(n) {
                    out.push(d);
                }
            }
            _ => {}
        }
    }
    out
}

// ── 1. self-assignment ───────────────────────────────────────────────────────

/// `x = x` or `this.x = this.x` — LHS and RHS are the *same* simple name / `this.`-qualified field,
/// byte-for-byte after trimming. Only plain `=` (a compound `x += x` is a real operation, never
/// flagged) and only shapes with no side effects (a bare identifier or `this.<ident>`), so an
/// array/index or arbitrary-expression LHS/RHS is skipped rather than risk a false positive.
fn self_assignment<'t>(assign: Node<'t>, bytes: &[u8]) -> Option<Diagnostic> {
    // The operator must be a plain `=`. Compound assignments (`+=`, `|=`, …) genuinely mutate.
    let op = assign.child_by_field_name("operator")?;
    if op.utf8_text(bytes).ok()? != "=" {
        return None;
    }
    let left = assign.child_by_field_name("left")?;
    let right = assign.child_by_field_name("right")?;

    // Both sides must be a side-effect-free simple reference: a bare identifier, or `this.<ident>`.
    if !is_simple_ref(left) || !is_simple_ref(right) {
        return None;
    }

    // Byte-for-byte identical (after trimming) → the two sides denote the same storage location.
    // Comparing text is safe here precisely because both are restricted to identifier / `this.ident`
    // shapes (no whitespace-sensitive sub-expressions that could differ semantically yet read alike).
    let lt = left.utf8_text(bytes).ok()?.trim();
    let rt = right.utf8_text(bytes).ok()?.trim();
    if lt != rt || lt.is_empty() {
        return None;
    }

    Some(crate::check_id::CheckId::SelfAssignment.at(assign, format!("Self-assignment of `{lt}` has no effect")))
}

/// Whether `node` is a side-effect-free simple reference we can safely text-compare: a bare
/// `identifier`, or a `field_access` of the form `this.<identifier>`.
fn is_simple_ref(node: Node) -> bool {
    match node.kind() {
        "identifier" => true,
        "field_access" => {
            // Require `this . <identifier>` — not `obj.f` (obj could be an arbitrary expression) and
            // not `this.a.b` (the object of the outer access wouldn't be `this`).
            let object = node.child_by_field_name("object");
            let field = node.child_by_field_name("field");
            matches!(object.map(|o| o.kind()), Some("this"))
                && matches!(field.map(|f| f.kind()), Some("identifier"))
        }
        _ => false,
    }
}

// ── 2. division / modulo by a literal zero ───────────────────────────────────

/// `a / 0` or `a % 0L` — the RHS is *literally* an integer-zero literal. We do NOT evaluate constant
/// expressions (only a bare literal counts) and we do NOT touch floating zero (`0.0` yields
/// Infinity/NaN, which is legal), so this can only fire on the genuinely-broken integer case.
fn div_mod_by_zero<'t>(bin: Node<'t>, bytes: &[u8]) -> Option<Diagnostic> {
    let op = bin.child_by_field_name("operator")?.utf8_text(bytes).ok()?;
    let noun = match op {
        "/" => "Division",
        "%" => "Modulo",
        _ => return None,
    };
    let right = bin.child_by_field_name("right")?;
    if !is_integer_zero_literal(right, bytes) {
        return None;
    }
    Some(crate::check_id::CheckId::DivisionByZero.at(bin, format!("{noun} by zero")))
}

/// Whether `node` is an integer literal whose value is zero (`0`, `0L`, `0x0`, `0b0`, `00`, `0_0`, …).
/// Restricted to the *integer* literal node kinds — a `decimal_floating_point_literal` like `0.0` is a
/// different kind and never matches, so floating zero is correctly left legal.
fn is_integer_zero_literal(node: Node, bytes: &[u8]) -> bool {
    if !matches!(
        node.kind(),
        "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal"
    ) {
        return false;
    }
    let Ok(text) = node.utf8_text(bytes) else { return false };
    is_zero_valued_int_literal(text)
}

/// True iff the integer-literal text denotes zero. Strips the optional `l`/`L` long suffix and the
/// `_` digit separators, then checks that every remaining digit is `0` — this covers `0`, `0L`, `00`,
/// `0x0`, `0b0`, `0_0` uniformly without having to parse each radix. A `0x…`/`0b…` prefix is handled
/// by ignoring the leading `0` + radix marker and confirming the payload digits are all zero.
fn is_zero_valued_int_literal(raw: &str) -> bool {
    let mut s = raw.trim();
    // Drop the long suffix.
    if let Some(stripped) = s.strip_suffix(['l', 'L']) {
        s = stripped;
    }
    // Normalise away digit separators.
    let s: String = s.chars().filter(|&c| c != '_').collect();
    if s.is_empty() {
        return false;
    }
    // Strip a radix prefix (0x / 0X / 0b / 0B). Note a bare `0`/`00` has no prefix and falls through.
    let digits = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(rest) => rest,
        None => match s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
            Some(rest) => rest,
            None => s.as_str(),
        },
    };
    // After removing any prefix, an empty payload means the whole literal was just the prefix (never
    // valid Java, so be conservative and don't flag). Otherwise every digit must be `0`.
    !digits.is_empty() && digits.chars().all(|c| c == '0')
}

// ── 3. empty statement ───────────────────────────────────────────────────────

/// Flag every stray `;` that appears directly as a statement inside `block`. In tree-sitter-java an
/// empty statement is the *anonymous* `;` token in the `statement` choice (there is no named
/// `empty_statement` node), so we must walk ALL children of the block — `collect_nodes` only records
/// named children and would miss it. Restricting to a `;` whose parent is a `block` is what makes this
/// safe: the `;` inside `for(;;)` are children of the `for_statement`, and a `while(cond);` body `;`
/// is a child of the loop — neither is a child of a `block`, so neither is ever flagged.
fn empty_statements_in_block(block: Node, out: &mut Vec<Diagnostic>) {
    let mut c = block.walk();
    for ch in block.children(&mut c) {
        // The anonymous `;` token's kind is the literal ";". The block's own `{`/`}` are also
        // anonymous but have distinct kinds, so this matches only genuine empty statements.
        if ch.kind() == ";" {
            out.push(crate::check_id::CheckId::EmptyStatement.at(ch, "Empty statement"));
        }
    }
}

// ── 4. reference equality with a String literal ──────────────────────────────

/// `"x" == s` / `s != "x"` — one operand is syntactically a `string_literal`. Comparing a String with
/// `==`/`!=` tests reference identity, not contents; a literal on either side is the tell-tale bug
/// that needs no type information (both `null` comparisons and non-literal `a == b` are left alone —
/// the latter may legitimately be reference equality on non-Strings).
fn string_reference_equality<'t>(bin: Node<'t>, bytes: &[u8]) -> Option<Diagnostic> {
    let op = bin.child_by_field_name("operator")?.utf8_text(bytes).ok()?;
    if op != "==" && op != "!=" {
        return None;
    }
    let left = bin.child_by_field_name("left")?;
    let right = bin.child_by_field_name("right")?;
    if left.kind() == "string_literal" || right.kind() == "string_literal" {
        return Some(crate::check_id::CheckId::StringReferenceEquality.at(
            bin,
            "Comparing strings with `==` compares references, not contents — use `.equals()`",
        ));
    }
    None
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

    /// Wrap `body` in a method and return the lint messages.
    fn lint(body: &str) -> Vec<String> {
        let src = format!("class C {{ int f; void m() {{ {body} }} }}");
        let tree = parse(&src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        expr_lint_warnings_nodes(&nodes, &src).into_iter().map(|d| d.message).collect()
    }

    // ── 1. self-assignment ───────────────────────────────────────────────────

    #[test]
    fn self_assignment_identifier_is_flagged() {
        let d = lint("int x = 0; x = x;");
        assert!(d.iter().any(|m| m.contains("Self-assignment of `x`")), "{d:?}");
    }

    #[test]
    fn self_assignment_this_field_is_flagged() {
        let d = lint("this.f = this.f;");
        assert!(d.iter().any(|m| m.contains("Self-assignment of `this.f`")), "{d:?}");
    }

    #[test]
    fn different_operands_not_flagged() {
        // The critical negative: `x = y` must never be flagged.
        let d = lint("int x = 0; int y = 1; x = y;");
        assert!(!d.iter().any(|m| m.contains("Self-assignment")), "{d:?}");
    }

    #[test]
    fn compound_self_assignment_not_flagged() {
        // `x += x` is a real doubling, not a no-op.
        let d = lint("int x = 2; x += x;");
        assert!(!d.iter().any(|m| m.contains("Self-assignment")), "{d:?}");
    }

    #[test]
    fn this_field_vs_bare_field_not_flagged() {
        // `this.f = f` may copy a parameter/local into the field — not a self-assignment.
        let d = lint("int f2 = 0; this.f = f2;");
        assert!(!d.iter().any(|m| m.contains("Self-assignment")), "{d:?}");
    }

    // ── 2. division / modulo by zero ─────────────────────────────────────────

    #[test]
    fn division_by_zero_is_flagged() {
        let d = lint("int a = 1; int b = a / 0;");
        assert!(d.iter().any(|m| m == "Division by zero"), "{d:?}");
    }

    #[test]
    fn modulo_by_zero_long_is_flagged() {
        let d = lint("long a = 1; long b = a % 0L;");
        assert!(d.iter().any(|m| m == "Modulo by zero"), "{d:?}");
    }

    #[test]
    fn hex_zero_is_flagged() {
        let d = lint("int a = 1; int b = a / 0x0;");
        assert!(d.iter().any(|m| m == "Division by zero"), "{d:?}");
    }

    #[test]
    fn division_by_nonzero_not_flagged() {
        let d = lint("int a = 1; int b = a / 2;");
        assert!(!d.iter().any(|m| m.contains("by zero")), "{d:?}");
    }

    #[test]
    fn division_by_variable_not_flagged() {
        // `a / b` — RHS isn't a literal, so we never guess.
        let d = lint("int a = 1; int b = 0; int c = a / b;");
        assert!(!d.iter().any(|m| m.contains("by zero")), "{d:?}");
    }

    #[test]
    fn division_by_float_zero_not_flagged() {
        // `a / 0.0` is Infinity/NaN — legal, must not be flagged.
        let d = lint("double a = 1.0; double b = a / 0.0;");
        assert!(!d.iter().any(|m| m.contains("by zero")), "{d:?}");
    }

    // ── 3. empty statement ───────────────────────────────────────────────────

    #[test]
    fn stray_semicolon_in_block_is_flagged() {
        let d = lint("int x = 0; ;");
        assert!(d.iter().any(|m| m == "Empty statement"), "{d:?}");
    }

    #[test]
    fn multiple_stray_semicolons_flagged_each() {
        let d = lint("; ;");
        assert_eq!(d.iter().filter(|m| *m == "Empty statement").count(), 2, "{d:?}");
    }

    #[test]
    fn normal_statement_terminator_not_flagged() {
        // The `;` that ends `int x = 0;` terminates a real statement — not an empty statement.
        let d = lint("int x = 0;");
        assert!(!d.iter().any(|m| m == "Empty statement"), "{d:?}");
    }

    #[test]
    fn for_loop_empty_clauses_not_flagged() {
        // The `;` inside `for(;;)` are children of the for_statement, not a block.
        let d = lint("for (int i = 0; i < 3; i++) { foo(); }");
        assert!(!d.iter().any(|m| m == "Empty statement"), "{d:?}");
    }

    #[test]
    fn empty_for_header_not_flagged() {
        let d = lint("for (;;) { break; }");
        assert!(!d.iter().any(|m| m == "Empty statement"), "{d:?}");
    }

    // ── 4. String reference equality ─────────────────────────────────────────

    #[test]
    fn string_literal_equality_is_flagged() {
        let d = lint("String s = \"\"; boolean b = s == \"x\";");
        assert!(d.iter().any(|m| m.contains("compares references")), "{d:?}");
    }

    #[test]
    fn string_literal_inequality_is_flagged() {
        let d = lint("String s = \"\"; boolean b = \"x\" != s;");
        assert!(d.iter().any(|m| m.contains("compares references")), "{d:?}");
    }

    #[test]
    fn equals_call_not_flagged() {
        // `foo.equals("x")` is the correct API — no binary `==`, nothing to flag.
        let d = lint("String foo = \"\"; boolean b = foo.equals(\"x\");");
        assert!(!d.iter().any(|m| m.contains("compares references")), "{d:?}");
    }

    #[test]
    fn non_literal_equality_not_flagged() {
        // `a == b` with no string literal — may be legitimate reference equality; never flagged.
        let d = lint("Object a = null; Object b = null; boolean r = a == b;");
        assert!(!d.iter().any(|m| m.contains("compares references")), "{d:?}");
    }

    #[test]
    fn null_comparison_not_flagged() {
        // `s == null` is the canonical null check — must stay silent.
        let d = lint("String s = null; boolean r = s == null;");
        assert!(!d.iter().any(|m| m.contains("compares references")), "{d:?}");
    }
}

// ── 4. empty `if` body ───────────────────────────────────────────────────────

/// `if (cond);` — the body is a bare `;`, so the statement the author meant to guard sits after the
/// `if` and runs either way. A typo whose damage is invisible at the site: the indentation below
/// still reads as a body.
///
/// Only the `;` shape. An empty BLOCK (`if (c) {}`) is left alone — it is how a deliberate "nothing
/// happens here" is written, often beside a commented-out line or an `else` that does the work.
fn empty_if_body(stmt: Node) -> Option<Diagnostic> {
    let body = stmt.child_by_field_name("consequence")?;
    (body.kind() == ";").then(|| {
        crate::check_id::CheckId::at(
            crate::check_id::CheckId::EmptyIfBody,
            body,
            "the `if` body is empty — the statement below it runs unconditionally",
        )
    })
}

#[cfg(test)]
mod empty_if_tests {
    use crate::check::collect_nodes;

    fn codes(src: &str) -> Vec<String> {
        let tree = bennu_java::prelude::parse_java(src).expect("parse");
        let nodes = collect_nodes(tree.root_node());
        super::expr_lint_warnings_nodes(&nodes, src).into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn a_semicolon_if_body_is_flagged() {
        let src = "class A { void m(boolean b) { if (b); doIt(); } void doIt() {} }";
        assert!(codes(src).contains(&"empty-if-body".to_string()));
    }

    #[test]
    fn an_empty_block_body_is_left_alone() {
        let src = "class A { void m(boolean b) { if (b) {} else { doIt(); } } void doIt() {} }";
        assert!(!codes(src).contains(&"empty-if-body".to_string()));
    }

    #[test]
    fn an_ordinary_if_is_left_alone() {
        let src = "class A { void m(boolean b) { if (b) doIt(); } void doIt() {} }";
        assert!(!codes(src).contains(&"empty-if-body".to_string()));
    }
}
