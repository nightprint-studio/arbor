//! Constant expressions (JLS §15.29) — for the few checks that must know a value the source writes as
//! an expression: two `case` labels that are one constant spelled twice, an `int` expression narrowed
//! implicitly because it is a constant that fits.
//!
//! A name is read as a constant variable only from what the enclosing scopes declare. A name nothing
//! here declares may still be one (an inherited field, a static import), so it folds to nothing and
//! is never [`certainly_not_constant`] either — both answers stay on the silent side.

use std::collections::HashMap;

use tree_sitter::Node;

use crate::support::nodes::has_keyword;

/// The value of a constant expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Folded {
    Int(i64),
    Str(String),
}

/// The names visible at a point that are constant variables — mapped to their initializer — and,
/// mapped to `None`, the names that shadow one without being one (parameters, non-`final` locals
/// and fields). Innermost scope wins.
pub(crate) type ConstantNames<'t> = HashMap<String, Option<Node<'t>>>;

/// Deeper than any constant a person writes; a cycle (`final int A = B, B = A`) ends here.
const MAX_DEPTH: u8 = 8;

/// The [`ConstantNames`] in scope at `at`.
pub(crate) fn constant_names<'t>(at: Node<'t>, bytes: &[u8]) -> ConstantNames<'t> {
    let mut names = ConstantNames::new();
    let note = |decl: Node<'t>, is_final: bool, names: &mut ConstantNames<'t>| {
        let mut c = decl.walk();
        for d in decl.named_children(&mut c).filter(|d| d.kind() == "variable_declarator") {
            if let Some(name) = d.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()) {
                let value = if is_final { d.child_by_field_name("value") } else { None };
                names.entry(name.to_string()).or_insert(value);
            }
        }
    };
    let mut cur = at.parent();
    while let Some(n) = cur {
        match n.kind() {
            "block" => {
                let mut c = n.walk();
                for stmt in n.named_children(&mut c).take_while(|s| s.start_byte() < at.start_byte()) {
                    if stmt.kind() == "local_variable_declaration" {
                        note(stmt, has_keyword(stmt, bytes, "final"), &mut names);
                    }
                }
            }
            "method_declaration" | "constructor_declaration" | "lambda_expression" => {
                if let Some(params) = n.child_by_field_name("parameters") {
                    if params.kind() == "identifier" {
                        if let Ok(name) = params.utf8_text(bytes) {
                            names.entry(name.to_string()).or_insert(None);
                        }
                    }
                    let mut c = params.walk();
                    for p in params.named_children(&mut c) {
                        let name = match p.kind() {
                            "identifier" => p.utf8_text(bytes).ok(),
                            _ => p.child_by_field_name("name").and_then(|x| x.utf8_text(bytes).ok()),
                        };
                        if let Some(name) = name {
                            names.entry(name.to_string()).or_insert(None);
                        }
                    }
                }
            }
            "class_body" | "enum_body_declarations" | "interface_body" => {
                let mut c = n.walk();
                for member in n.named_children(&mut c) {
                    match member.kind() {
                        "field_declaration" => note(member, has_keyword(member, bytes, "final"), &mut names),
                        // Interface fields are implicitly `static final`.
                        "constant_declaration" => note(member, true, &mut names),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        cur = n.parent();
    }
    names
}

/// The value of `n` when it is a constant expression built from integer, character and string
/// literals, `+ - * / %`, a sign, parentheses and constant variables. `None` for anything else.
pub(crate) fn fold(n: Node, bytes: &[u8], names: &ConstantNames, depth: u8) -> Option<Folded> {
    if depth > MAX_DEPTH {
        return None;
    }
    let text = n.utf8_text(bytes).ok()?;
    match n.kind() {
        "decimal_integer_literal" | "hex_integer_literal" | "octal_integer_literal" | "binary_integer_literal" => {
            parse_int_literal(text).map(Folded::Int)
        }
        "character_literal" => {
            let inner = text.strip_prefix('\'')?.strip_suffix('\'')?;
            let mut chars = inner.chars();
            let c = chars.next().filter(|c| *c != '\\')?;
            chars.next().is_none().then_some(Folded::Int(c as i64))
        }
        "string_literal" => {
            let inner = text.strip_prefix('"')?.strip_suffix('"')?;
            (!inner.contains('\\')).then(|| Folded::Str(inner.to_string()))
        }
        "parenthesized_expression" => fold(n.named_child(0)?, bytes, names, depth + 1),
        "unary_expression" => {
            let op = n.child_by_field_name("operator")?.utf8_text(bytes).ok()?;
            match (op, fold(n.child_by_field_name("operand")?, bytes, names, depth + 1)?) {
                ("-", Folded::Int(v)) => v.checked_neg().map(Folded::Int),
                ("+", Folded::Int(v)) => Some(Folded::Int(v)),
                _ => None,
            }
        }
        "binary_expression" => {
            let op = n.child_by_field_name("operator")?.utf8_text(bytes).ok()?;
            let left = fold(n.child_by_field_name("left")?, bytes, names, depth + 1)?;
            let right = fold(n.child_by_field_name("right")?, bytes, names, depth + 1)?;
            match (left, right) {
                (Folded::Int(a), Folded::Int(b)) => match op {
                    "+" => a.checked_add(b),
                    "-" => a.checked_sub(b),
                    "*" => a.checked_mul(b),
                    "/" => a.checked_div(b),
                    "%" => a.checked_rem(b),
                    _ => None,
                }
                .map(Folded::Int),
                (Folded::Str(a), Folded::Str(b)) if op == "+" => Some(Folded::Str(a + &b)),
                _ => None,
            }
        }
        "identifier" => fold((*names.get(text)?)?, bytes, names, depth + 1),
        _ => None,
    }
}

/// Whether `n` can be shown NOT to be a constant expression: somewhere in it is a call, an
/// assignment, an object — or a name bound to a parameter or a non-`final` variable.
pub(crate) fn certainly_not_constant(n: Node, bytes: &[u8], names: &ConstantNames, depth: u8) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    match n.kind() {
        "method_invocation" | "object_creation_expression" | "array_creation_expression" | "array_access"
        | "assignment_expression" | "update_expression" | "instanceof_expression" | "lambda_expression"
        | "method_reference" | "this" | "super" | "null_literal" | "class_literal" | "switch_expression" => true,
        "identifier" => match n.utf8_text(bytes).ok().and_then(|t| names.get(t)) {
            Some(None) => true,
            Some(Some(init)) => certainly_not_constant(*init, bytes, names, depth + 1),
            None => false,
        },
        // `Type.NAME` may be a constant of another type; nothing here can say.
        "field_access" => false,
        _ => {
            let mut c = n.walk();
            let found = n.named_children(&mut c).any(|ch| certainly_not_constant(ch, bytes, names, depth + 1));
            found
        }
    }
}

/// A Java integer literal (decimal, hex, octal, binary; underscores and an `L` suffix allowed).
fn parse_int_literal(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|c| *c != '_').collect();
    let t = t.trim_end_matches(['l', 'L']);
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok();
    }
    if let Some(bin) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return i64::from_str_radix(bin, 2).ok();
    }
    if t.len() > 1 && t.starts_with('0') && t.bytes().all(|b| b.is_ascii_digit()) {
        return i64::from_str_radix(&t[1..], 8).ok();
    }
    t.parse::<i64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fold the initializer of the last local declared in `body`.
    fn value_of_last(body: &str) -> (Option<Folded>, bool) {
        let src = format!("class C {{ static final int K = 10; int mutable = 1; void m(int p) {{ {body} }} }}");
        let tree = bennu_java::prelude::parse_java(&src).unwrap();
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        let value = nodes
            .iter()
            .filter(|n| n.kind() == "variable_declarator")
            .filter_map(|d| d.child_by_field_name("value"))
            .max_by_key(|v| v.start_byte())
            .unwrap();
        let names = constant_names(value, src.as_bytes());
        (fold(value, src.as_bytes(), &names, 0), certainly_not_constant(value, src.as_bytes(), &names, 0))
    }

    #[test]
    fn literals_and_arithmetic_fold() {
        assert_eq!(value_of_last("char c = 'a' + 1;").0, Some(Folded::Int(98)));
        assert_eq!(value_of_last("int x = (1 + 2) * 3;").0, Some(Folded::Int(9)));
        assert_eq!(value_of_last("String s = \"a\" + \"b\";").0, Some(Folded::Str("ab".into())));
        assert_eq!(value_of_last("int x = 0x10;").0, Some(Folded::Int(16)));
    }

    #[test]
    fn constant_variables_fold() {
        assert_eq!(value_of_last("byte b = K;").0, Some(Folded::Int(10)));
        assert_eq!(value_of_last("final int local = 20; byte b = local;").0, Some(Folded::Int(20)));
    }

    #[test]
    fn variables_are_certainly_not_constant() {
        assert!(value_of_last("byte b = p;").1);
        assert!(value_of_last("byte b = mutable;").1);
        assert!(value_of_last("int n = 1; byte b = n + 1;").1);
        // An unknown name may be a constant elsewhere.
        let (folded, not_constant) = value_of_last("byte b = ELSEWHERE;");
        assert!(folded.is_none() && !not_constant);
    }
}
