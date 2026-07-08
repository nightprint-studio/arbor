//! Constructor-shape diagnostics (pure-AST).
//!
//! **Method named like its class/enum** (`warning`): a `method_declaration` whose name equals its
//! enclosing class/enum simple name. The author almost certainly meant a constructor, but the return
//! type demotes it to a regular method that Java accepts silently — a classic, hard-to-spot slip. A
//! *real* constructor parses as `constructor_declaration` (no return type), a different node kind, so
//! it can never match here.
//!
//! Conservative (never a false positive): fires ONLY when the enclosing type is a `class`/`enum` AND
//! the method name string is byte-for-byte the type name string. Interfaces/records/annotations are
//! excluded (they can't declare constructors, so a same-named method isn't the same mistake).
//!
//! Note on the sibling "`this()`/`super()` must be first" and "can't call both `this()` and
//! `super()`" rules: tree-sitter-java's grammar only accepts an explicit constructor invocation as
//! the FIRST statement of a constructor body, so a misplaced or second chain call parses as an
//! `ERROR` node — which the syntax pass ([`crate::syntax`]) already reports. A dedicated check here
//! would be dead (it would never see a well-formed node for the illegal case), so it's intentionally
//! omitted rather than duplicating the syntax error.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Type declarations whose body can hold members. Used to find a method's enclosing type.
const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// Constructor-shape diagnostics over the shared pre-collected node list (one traversal across all
/// pure-AST checks).
pub fn ctor_check_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "method_declaration" {
            check_method_named_like_class(n, bytes, &mut out);
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// Flag a `method_declaration` whose name equals the simple name of its enclosing class/enum — a
/// return type turns an intended constructor into a silent method.
fn check_method_named_like_class(method: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(name_node) = method.child_by_field_name("name") else { return };
    let Ok(method_name) = name_node.utf8_text(bytes) else { return };

    let Some(ty) = enclosing_type(method) else { return };
    // Only class/enum: on those a same-named method is the classic "forgot the constructor" slip.
    if !matches!(ty.kind(), "class_declaration" | "enum_declaration") {
        return;
    }
    let Some(ty_name_node) = ty.child_by_field_name("name") else { return };
    let Ok(ty_name) = ty_name_node.utf8_text(bytes) else { return };

    if method_name == ty_name {
        out.push(Diagnostic {
            message: format!(
                "Method `{method_name}` has the same name as the class — did you mean a constructor? \
                 (a constructor has no return type)"
            ),
            severity: "warning".to_string(),
            code: String::new(),
            start: name_node.start_byte(),
            end: name_node.end_byte(),
        });
    }
}

/// The nearest enclosing type declaration of `node`, if any (mirrors `declarations::enclosing_type`).
fn enclosing_type(node: Node) -> Option<Node> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if TYPE_DECLS.contains(&n.kind()) {
            return Some(n);
        }
        cur = n.parent();
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

    fn diags(src: &str) -> Vec<Diagnostic> {
        let tree = parse(src);
        let nodes = crate::check::collect_nodes(tree.root_node());
        ctor_check_errors_nodes(&nodes, src)
    }

    fn msgs(src: &str) -> Vec<String> {
        diags(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn method_named_like_class_with_return_type_is_flagged() {
        let m = msgs("class Foo { public void Foo() {} }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("same name as the class"), "{m:?}");
        assert!(m[0].contains("`Foo`"), "{m:?}");
    }

    #[test]
    fn method_named_like_class_returning_value_is_flagged() {
        // A non-void return type is the same slip.
        let m = msgs("class Bar { int Bar() { return 0; } }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("did you mean a constructor"), "{m:?}");
    }

    #[test]
    fn method_named_like_enum_is_flagged() {
        let m = msgs("enum E { A; void E() {} }");
        assert!(m.iter().any(|s| s.contains("same name as the class")), "{m:?}");
    }

    #[test]
    fn real_constructor_is_not_flagged() {
        // `Foo() {}` with no return type parses as `constructor_declaration`, not `method_declaration`.
        let d = diags("class Foo { Foo() {} }");
        assert!(
            d.iter().all(|d| !d.message.contains("same name as the class")),
            "a real constructor must not be flagged: {:?}",
            d.iter().map(|d| &d.message).collect::<Vec<_>>(),
        );
    }

    #[test]
    fn ordinary_method_is_not_flagged() {
        // A normal getter whose name differs from the class must stay clean.
        assert!(msgs("class Foo { public int getValue() { return 0; } }").is_empty());
    }

    #[test]
    fn same_named_method_in_interface_is_not_flagged() {
        // Excluded on purpose: interfaces can't declare constructors, so this isn't the same slip.
        assert!(msgs("interface Foo { void Foo(); }").is_empty());
    }

    #[test]
    fn nested_class_method_matches_its_own_type_not_the_outer() {
        // The method's enclosing type is `Inner`, so a method named `Outer` does NOT match.
        assert!(msgs("class Outer { class Inner { void Outer() {} } }").is_empty());
        // …but a method named `Inner` inside `Inner` does.
        let m = msgs("class Outer { class Inner { void Inner() {} } }");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("`Inner`"), "{m:?}");
    }

    #[test]
    fn plain_constructor_is_clean() {
        assert!(diags("class X { X() { int a = 1; } }").is_empty());
    }
}
