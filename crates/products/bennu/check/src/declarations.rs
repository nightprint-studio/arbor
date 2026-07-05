//! Declaration & modifier legality — pure-AST rules that don't need a resolver.
//!
//! These are structural Java rules the compiler enforces regardless of types, so they're
//! false-positive-free from the syntax tree alone:
//!   * an `abstract` method only in an `abstract` class or an interface (never a concrete class);
//!   * `abstract` methods carry no body; a `default` method only in an interface;
//!   * illegal modifier combinations (two visibility modifiers, `abstract` + `private`/`static`/
//!     `final`/`native`, a class that's `abstract` *and* `final`, `final` + `volatile` field);
//!   * a `record` can't be `abstract` and can't declare instance fields;
//!   * an `enum` constant with arguments needs a constructor.

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

const TYPE_DECLS: [&str; 5] = [
    "class_declaration",
    "interface_declaration",
    "enum_declaration",
    "record_declaration",
    "annotation_type_declaration",
];

/// All declaration/modifier legality errors in `root`.
pub fn declaration_errors(root: Node, source: &str) -> Vec<Diagnostic> {
    declaration_errors_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks).
pub fn declaration_errors_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        match n.kind() {
            "method_declaration" => check_method(n, bytes, &mut out),
            "class_declaration" => check_class(n, bytes, &mut out),
            "record_declaration" => check_record(n, bytes, &mut out),
            "enum_declaration" => check_enum(n, bytes, &mut out),
            "field_declaration" => check_field(n, bytes, &mut out),
            _ => {}
        }
    }
    out
}

/// The keyword modifiers (anonymous tokens) on a declaration — `["public", "abstract"]`. Annotations
/// (named nodes inside `modifiers`) are excluded.
fn modifier_keywords<'a>(node: Node, bytes: &'a [u8]) -> Vec<&'a str> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "modifiers" {
            let mut out = Vec::new();
            let mut mc = ch.walk();
            for m in ch.children(&mut mc) {
                if !m.is_named() {
                    if let Ok(t) = m.utf8_text(bytes) {
                        out.push(t);
                    }
                }
            }
            return out;
        }
    }
    Vec::new()
}

/// The nearest enclosing type declaration of `node`, if any.
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

fn err(node: Node, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        message: message.into(),
        severity: "error".to_string(),
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

/// The span to anchor a declaration error on: its NAME token (tighter than the whole decl).
fn name_span(node: Node) -> Node {
    node.child_by_field_name("name").unwrap_or(node)
}

fn visibility_count(mods: &[&str]) -> usize {
    mods.iter().filter(|m| matches!(**m, "public" | "private" | "protected")).count()
}

fn check_method(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    let has = |m: &str| mods.contains(&m);
    let is_abstract = has("abstract");
    let has_body = n.child_by_field_name("body").map(|b| b.kind() == "block").unwrap_or(false);
    let anchor = name_span(n);

    if visibility_count(&mods) > 1 {
        out.push(err(anchor, "Illegal combination of modifiers: only one of public/protected/private"));
    }
    if is_abstract {
        for bad in ["private", "static", "final", "native", "synchronized"] {
            if has(bad) {
                out.push(err(anchor, format!("Illegal combination of modifiers: abstract and {bad}")));
            }
        }
        if has_body {
            out.push(err(anchor, "Abstract method cannot have a body"));
        }
        // Placement: only concrete CLASSES are wrong (interfaces allow abstract; enums/records are
        // left to avoid a false positive on an enum's constant-bodied abstract method).
        if let Some(ty) = enclosing_type(n) {
            if ty.kind() == "class_declaration" && !modifier_keywords(ty, bytes).contains(&"abstract") {
                out.push(err(anchor, "Abstract method in non-abstract class"));
            }
        }
    }
    if has("default") {
        let in_interface = enclosing_type(n).map(|t| t.kind() == "interface_declaration").unwrap_or(false);
        if !in_interface {
            out.push(err(anchor, "Default methods are only allowed in interfaces"));
        }
    }
}

fn check_class(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    if mods.contains(&"abstract") && mods.contains(&"final") {
        out.push(err(name_span(n), "Illegal combination of modifiers: abstract and final"));
    }
}

fn check_record(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let mods = modifier_keywords(n, bytes);
    if mods.contains(&"abstract") {
        out.push(err(name_span(n), "A record cannot be abstract"));
    }
    // Instance fields: a record's state is its components — only static fields are allowed in the body.
    if let Some(body) = n.child_by_field_name("body") {
        let mut c = body.walk();
        for member in body.named_children(&mut c) {
            if member.kind() == "field_declaration"
                && !modifier_keywords(member, bytes).contains(&"static")
            {
                out.push(err(member, "Records cannot declare instance fields"));
            }
        }
    }
}

fn check_enum(n: Node, _bytes: &[u8], out: &mut Vec<Diagnostic>) {
    let Some(body) = n.child_by_field_name("body") else { return };
    let mut has_ctor = false;
    let mut arg_constant: Option<Node> = None;
    let mut c = body.walk();
    for member in body.named_children(&mut c) {
        match member.kind() {
            "enum_constant" => {
                if member.child_by_field_name("arguments").is_some() && arg_constant.is_none() {
                    arg_constant = Some(member);
                }
            }
            "enum_body_declarations" => {
                let mut dc = member.walk();
                for d in member.named_children(&mut dc) {
                    if d.kind() == "constructor_declaration" {
                        has_ctor = true;
                    }
                }
            }
            "constructor_declaration" => has_ctor = true,
            _ => {}
        }
    }
    if let (Some(constant), false) = (arg_constant, has_ctor) {
        out.push(err(
            name_span(constant),
            "Enum constant has arguments but the enum declares no matching constructor",
        ));
    }
}

fn check_field(n: Node, bytes: &[u8], out: &mut Vec<Diagnostic>) {
    // Only class/interface members — a record's instance fields are handled in `check_record`, and a
    // local variable declaration is a different node (`local_variable_declaration`).
    let mods = modifier_keywords(n, bytes);
    if visibility_count(&mods) > 1 {
        out.push(err(n, "Illegal combination of modifiers: only one of public/protected/private"));
    }
    if mods.contains(&"final") && mods.contains(&"volatile") {
        out.push(err(n, "Illegal combination of modifiers: final and volatile"));
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

    fn errs(src: &str) -> Vec<String> {
        let tree = parse(src);
        declaration_errors(tree.root_node(), src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn abstract_method_in_concrete_class_is_flagged() {
        let e = errs("class C { abstract void m(); }");
        assert!(e.iter().any(|m| m.contains("non-abstract class")), "{e:?}");
    }

    #[test]
    fn abstract_method_in_abstract_class_is_ok() {
        assert!(errs("abstract class C { abstract void m(); }").is_empty());
    }

    #[test]
    fn abstract_method_in_interface_is_ok() {
        assert!(errs("interface I { void m(); abstract void n(); }").is_empty());
    }

    #[test]
    fn abstract_method_with_body_is_flagged() {
        let e = errs("abstract class C { abstract void m() {} }");
        assert!(e.iter().any(|m| m.contains("cannot have a body")), "{e:?}");
    }

    #[test]
    fn default_method_in_class_is_flagged() {
        let e = errs("class C { default void m() {} }");
        assert!(e.iter().any(|m| m.contains("only allowed in interfaces")), "{e:?}");
    }

    #[test]
    fn default_method_in_interface_is_ok() {
        assert!(errs("interface I { default void m() {} }").is_empty());
    }

    #[test]
    fn abstract_final_class_is_flagged() {
        let e = errs("abstract final class C {}");
        assert!(e.iter().any(|m| m.contains("abstract and final")), "{e:?}");
    }

    #[test]
    fn abstract_private_method_is_flagged() {
        let e = errs("abstract class C { abstract private void m(); }");
        assert!(e.iter().any(|m| m.contains("abstract and private")), "{e:?}");
    }

    #[test]
    fn two_visibility_modifiers_flagged() {
        let e = errs("class C { public private void m() {} }");
        assert!(e.iter().any(|m| m.contains("only one of")), "{e:?}");
    }

    #[test]
    fn record_cannot_be_abstract() {
        let e = errs("abstract record R(int x) {}");
        assert!(e.iter().any(|m| m.contains("record cannot be abstract")), "{e:?}");
    }

    #[test]
    fn record_instance_field_flagged_static_ok() {
        let e = errs("record R(int x) { int y; static int Z = 1; }");
        assert_eq!(
            e.iter().filter(|m| m.contains("instance fields")).count(),
            1,
            "only the non-static field y is flagged: {e:?}",
        );
    }

    #[test]
    fn enum_constant_with_args_needs_constructor() {
        let e = errs("enum E { RED(255), GREEN(0); }");
        assert!(e.iter().any(|m| m.contains("no matching constructor")), "{e:?}");
    }

    #[test]
    fn enum_constant_with_args_and_constructor_is_ok() {
        let src = "enum E { RED(255); private final int v; E(int v) { this.v = v; } }";
        assert!(errs(src).iter().all(|m| !m.contains("constructor")), "{:?}", errs(src));
    }

    #[test]
    fn plain_enum_without_args_is_ok() {
        assert!(errs("enum E { A, B, C }").is_empty());
    }

    #[test]
    fn final_volatile_field_flagged() {
        let e = errs("class C { final volatile int x = 0; }");
        assert!(e.iter().any(|m| m.contains("final and volatile")), "{e:?}");
    }

    #[test]
    fn ordinary_class_is_clean() {
        let src = "public class C { private int x; public int get() { return x; } }";
        assert!(errs(src).is_empty(), "{:?}", errs(src));
    }
}
