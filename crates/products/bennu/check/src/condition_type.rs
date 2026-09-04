//! Condition-type diagnostics — a control-flow condition that is DEFINITELY not `boolean`.
//!
//! In Java the condition of `if` / `while` / `do`…`while` / the classic `for`, and the middle
//! expression of a ternary `?:`, must be `boolean` (or `java.lang.Boolean`, which auto-unboxes).
//! `if (5)`, `if ("x")`, `while (someInt)` are compile errors. This resolver-backed check infers
//! the condition expression's type (via the same shared inference the cast/assignment check uses)
//! and flags ONLY when that type resolves to a DEFINITE non-boolean.
//!
//! PARAMOUNT — never a false positive. An unresolvable / uninferable condition is "unknown", not an
//! error. We flag only when inference returns a concrete type that is provably NOT boolean:
//!   * a primitive that isn't `boolean` (`int`/`long`/`double`/`char`/…);
//!   * `java/lang/String`;
//!   * a resolved class binary that is NOT `java/lang/Boolean` (a real class the resolver knows).
//! If inference yields `None`, or `boolean`, or `java/lang/Boolean` (unboxes), or a binary the
//! resolver can't confirm as a real class, we SKIP.

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::nodes::{is_primitive, is_type_var, simple_name};


/// Parse `source` and flag control-flow conditions whose inferred type is definitely non-boolean.
pub fn condition_type_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::check::collect_nodes(root);
    condition_type_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` + reuses `root` (for inference) + `symbols` + the
/// shared per-file inference `cache`. Signature mirrors [`crate::casts::type_compat_errors_in`].
pub fn condition_type_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        // The boolean-position sub-node lives in the `condition` field of each of these kinds.
        // `if_statement`, `while_statement`, `do_statement`, `for_statement` (classic `for`) and
        // `ternary_expression` all name it `condition`. An enhanced-for has no `condition` field, so
        // it's naturally skipped.
        let condition_holder = match n.kind() {
            "if_statement" | "while_statement" | "do_statement" | "for_statement"
            | "ternary_expression" => n.child_by_field_name("condition"),
            _ => None,
        };
        let Some(holder) = condition_holder else { continue };
        // The `condition` in tree-sitter-java is usually a `parenthesized_expression` wrapping the
        // real expression (mirrors how `switches::selector_primitive` unwraps `(expr)`). Infer on the
        // inner expression so `(5)` types as the `int` literal, not the parenthesized node.
        let cond = unwrap_parens(holder);

        // Infer the condition expression's static type. SKIP when inference yields nothing — an
        // uninferable / unresolvable condition is "unknown", never an error.
        let Some(ty) = infer_node_type_cached(&root, source, symbols, &cond, resolver, cache) else {
            continue;
        };
        // SKIP an empty binary name (inference produced no usable type token).
        if ty.binary_name.is_empty() {
            continue;
        }
        if let Some(actual) = definite_non_boolean(&ty.binary_name, resolver) {
            out.push(err(
                format!("Incompatible types: `{actual}` cannot be converted to `boolean`"),
                cond,
            ));
        }
    }
    out
}

/// Unwrap a `parenthesized_expression` to its inner expression; return `node` unchanged otherwise.
/// The `condition` field of `if`/`while`/`for` is nearly always `(expr)`; a ternary's is the bare
/// expression. A `parenthesized_expression` with no named child (shouldn't happen for a real
/// condition) falls back to the node itself so we never lose the position.
fn unwrap_parens(node: Node) -> Node {
    if node.kind() == "parenthesized_expression" {
        // Explicit loop (not `.named_child(0)` chained with a default) to stay clear of the
        // borrow-checker gotcha and keep the fallback obvious.
        let mut c = node.walk();
        for ch in node.named_children(&mut c) {
            return ch; // the first (only) named child is the real expression
        }
    }
    node
}

/// The display name of `binary` when it is a DEFINITE non-boolean type, or `None` when it is
/// `boolean` / `java/lang/Boolean` / unknown (→ SKIP, never a false positive).
///
///   * `"boolean"` — the correct type → `None`.
///   * `"java/lang/Boolean"` — auto-unboxes to `boolean`, so NEVER flagged → `None`.
///   * another primitive token (`"int"`, `"long"`, `"double"`, `"char"`, …) → definite mismatch.
///   * `"java/lang/String"` → definite mismatch.
///   * any other slash-form binary that the resolver confirms is a real class → definite mismatch.
///   * a binary the resolver can't confirm (un-indexed / a type variable / an array) → `None`.
fn definite_non_boolean(binary: &str, resolver: &dyn TypeResolver) -> Option<String> {
    // The correct type, and its box that unboxes to it — both fine.
    if binary == "boolean" || binary == "java/lang/Boolean" {
        return None;
    }
    // Any OTHER primitive is a definite non-boolean (`int`, `long`, `double`, `char`, …). Note that
    // `void` can't appear as a real condition value; if it somehow did it's still not boolean, but a
    // `void` condition would be a parse/type error upstream — harmless to name it.
    if is_primitive(binary) {
        return Some(binary.to_string());
    }
    // `String` is a definite non-boolean (`if ("x")`).
    if binary == "java/lang/String" {
        return Some("String".to_string());
    }
    // A type variable (`"T"`) or an array (`"int[]"`) is NOT something we can confirm as a definite
    // non-boolean concrete class here — SKIP (a bound could, in theory, be Boolean; an array is never
    // a valid condition but that's a different error, don't claim a boolean mismatch).
    if is_type_var(binary) || binary.ends_with("[]") {
        return None;
    }
    // Otherwise: flag ONLY when the resolver confirms this is a REAL class it knows (its members
    // resolve). An un-indexed / unknown binary → SKIP (could conceivably be an alias we don't model).
    // We already excluded `java/lang/Boolean` above, so any resolvable class here is a definite
    // non-boolean.
    resolver.members_of(binary)?;
    Some(simple_name(binary).to_string())
}

fn err(message: String, node: Node) -> Diagnostic {
    crate::check_id::CheckId::NonBooleanCondition.at(node, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, Member, TypeRef};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// The same mock resolver shape used by `casts.rs` — a fixed map of binary → members and a
    /// simple-name → binary table.
    struct MapResolver {
        members: HashMap<String, ClassMembers>,
        simple: HashMap<String, String>,
    }

    impl TypeResolver for MapResolver {
        fn members_of(&self, binary: &str) -> Option<Arc<ClassMembers>> {
            self.members.get(binary).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, name: &str, _imports: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn method(name: &str, ret: &str) -> Member {
        Member::method(name, TypeRef::simple(ret.to_string()), Vec::new())
    }

    fn cls(superclass: Option<&str>, methods: Vec<Member>, fields: Vec<Member>) -> ClassMembers {
        ClassMembers {
            type_params: Vec::new(),
            superclass: superclass.map(TypeRef::simple),
            interfaces: Vec::new(),
            methods,
            fields,
            flags: ClassFlags::default(),
        }
    }

    /// A `Gadget` with `isReady()->boolean`; a `Provider` with `flag` (`boolean` field), `count`
    /// (`int` field), `obj` (`Gadget` field). String/Boolean/Object modelled so the resolver can
    /// confirm real classes.
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), cls(None, vec![], vec![]));
        members.insert("java/lang/String".to_string(), cls(Some("java/lang/Object"), vec![], vec![]));
        members.insert("java/lang/Boolean".to_string(), cls(Some("java/lang/Object"), vec![], vec![]));
        members.insert(
            "com/acme/Gadget".to_string(),
            cls(Some("java/lang/Object"), vec![method("isReady", "boolean")], vec![]),
        );
        members.insert(
            "com/acme/Provider".to_string(),
            cls(
                Some("java/lang/Object"),
                vec![],
                vec![
                    Member::field("flag", TypeRef::simple("boolean".to_string())),
                    Member::field("count", TypeRef::simple("int".to_string())),
                    Member::field("obj", TypeRef::simple("com/acme/Gadget".to_string())),
                ],
            ),
        );
        let simple = [
            ("Object", "java/lang/Object"),
            ("String", "java/lang/String"),
            ("Boolean", "java/lang/Boolean"),
            ("Gadget", "com/acme/Gadget"),
            ("Provider", "com/acme/Provider"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    /// Run the check over `body` placed inside a `Provider`-carrying class, returning the messages.
    fn diags(body: &str) -> Vec<String> {
        let src = format!("class C {{ Provider p; void m() {{ {body} }} }}");
        condition_type_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── positives: DEFINITE non-boolean conditions ─────────────────────────────

    #[test]
    fn if_int_literal_is_flagged() {
        let d = diags("if (5) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`") && d[0].contains("`boolean`"), "{d:?}");
    }

    #[test]
    fn while_int_expression_is_flagged() {
        // A `while` whose condition is an `int` field → a non-boolean condition.
        let d = diags("while (p.count) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn if_int_field_is_flagged() {
        let d = diags("if (p.count) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn if_string_literal_is_flagged() {
        let d = diags("if (\"x\") {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`String`"), "{d:?}");
    }

    // ── negatives: boolean / Boolean / unknown → SKIP ──────────────────────────

    #[test]
    fn if_true_is_ok() {
        // `true` types as boolean → no flag.
        assert!(diags("if (true) {}").is_empty());
    }

    #[test]
    fn comparison_condition_is_ok() {
        // `x > 0` is a boolean comparison — inferable and correct.
        assert!(diags("int x = 0; if (x > 0) {}").is_empty());
    }

    #[test]
    fn boolean_field_condition_is_ok() {
        // `p.flag` is a boolean field → no flag.
        assert!(diags("if (p.flag) {}").is_empty());
    }

    #[test]
    fn boolean_method_condition_is_ok() {
        // `p.obj.isReady()` returns boolean → no flag.
        assert!(diags("if (p.obj.isReady()) {}").is_empty());
    }

    #[test]
    fn uninferable_condition_is_not_flagged() {
        // `mystery()` has no known type → unknown, not an error.
        assert!(diags("if (mystery()) {}").is_empty());
    }

    #[test]
    fn boxed_boolean_is_not_flagged() {
        // A `Boolean` local auto-unboxes to boolean → NEVER flagged.
        assert!(diags("Boolean b = null; if (b) {}").is_empty());
    }

    // ── other condition-bearing constructs ─────────────────────────────────────

    #[test]
    fn ternary_int_condition_is_flagged() {
        // The middle expression of `?:` must be boolean; here it's an int field.
        let d = diags("int y = p.count ? 1 : 2;");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn for_int_condition_is_flagged() {
        let d = diags("for (int i = 0; p.count; i++) {}");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn do_while_int_condition_is_flagged() {
        let d = diags("do {} while (p.count);");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("`int`"), "{d:?}");
    }

    #[test]
    fn enhanced_for_is_not_flagged() {
        // `for (String s : xs)` has no boolean condition field → nothing to check.
        assert!(diags("java.util.List<String> xs = null; for (String s : xs) {}").is_empty());
    }
}
