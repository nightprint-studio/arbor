//! Duplicate-signature diagnostics — two methods or two constructors in the same type with the
//! **same name and the same parameter types**. Pure AST: signatures are compared by their written
//! parameter-type text, so `f(int)` twice is flagged while `f(int)` / `f(String)` (a legal overload)
//! is not.
//!
//! Conservative: comparison is by source text (generics kept), so it only reports an *exact*
//! duplicate — never a subtle erasure clash (`f(List<String>)` vs `f(List<Integer>)`), which stays
//! silent rather than risk a false positive.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

/// Flag duplicate method / constructor signatures within each type.
pub fn duplicate_signatures(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    duplicate_signatures_in(tree.root_node(), source)
}

/// Tree-driven core.
pub fn duplicate_signatures_in(root: Node, source: &str) -> Vec<Diagnostic> {
    duplicate_signatures_nodes(&crate::check::collect_nodes(root), source)
}

/// Slice-driven core (shared pre-collected node list — one traversal across all pure-AST checks). The
/// pre-order of the slice matches the old DFS, so the first-seen-wins dedup keys identically.
pub fn duplicate_signatures_nodes(nodes: &[Node], source: &str) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    // Key: (enclosing body node id, member kind + name, parameter type texts). First-seen node kept;
    // a second insertion is a duplicate.
    let mut seen: HashMap<(usize, String, Vec<String>), ()> = HashMap::new();
    let mut out = Vec::new();
    for &n in nodes {
        let (kind_name, name_node) = match n.kind() {
            "method_declaration" => {
                let Some(name) = n.child_by_field_name("name") else { continue };
                let Ok(t) = name.utf8_text(bytes) else { continue };
                (format!("m:{t}"), name)
            }
            "constructor_declaration" => {
                let Some(name) = n.child_by_field_name("name") else { continue };
                // Constructors of one class share a name; the params discriminate overloads.
                ("ctor".to_string(), name)
            }
            _ => continue,
        };
        let Some(body) = n.parent() else { continue };
        let params = param_types(n, bytes);
        let key = (body.id(), kind_name, params);
        if seen.insert(key, ()).is_some() {
            let what = if n.kind() == "constructor_declaration" { "constructor" } else { "method" };
            out.push(Diagnostic {
                message: format!("Duplicate {what}: another with the same signature is already declared"),
                severity: crate::check_id::CheckId::DuplicateMethod.severity().to_string(),
                code: crate::check_id::CheckId::DuplicateMethod.code().to_string(),
                start: name_node.start_byte(),
                end: name_node.end_byte(),
            });
        }
    }
    out.sort_by_key(|d| d.start);
    out
}

/// The written parameter types of a method/constructor, whitespace-normalised. A varargs `T...`
/// keeps its `...` so `f(T...)` and `f(T)` stay distinct.
fn param_types(member: Node, bytes: &[u8]) -> Vec<String> {
    let Some(params) = member.child_by_field_name("parameters") else { return Vec::new() };
    let mut out = Vec::new();
    let mut c = params.walk();
    for p in params.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                if let Some(t) = p.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) {
                    out.push(normalize(t));
                }
            }
            "spread_parameter" => {
                // `T... xs` — the type is the first type child; mark it varargs.
                let mut sc = p.walk();
                for ch in p.named_children(&mut sc) {
                    let k = ch.kind();
                    if k.ends_with("type") || k == "type_identifier" || k == "scoped_type_identifier"
                        || k == "generic_type" || k == "array_type"
                    {
                        if let Ok(t) = ch.utf8_text(bytes) {
                            out.push(format!("{}...", normalize(t)));
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dups(src: &str) -> Vec<String> {
        duplicate_signatures(src).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn duplicate_method_is_flagged() {
        let d = dups("class C { void f(int x) {} void f(int y) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Duplicate method"), "{d:?}");
    }

    #[test]
    fn legal_overload_is_ok() {
        assert!(dups("class C { void f(int x) {} void f(String y) {} }").is_empty());
    }

    #[test]
    fn duplicate_constructor_is_flagged() {
        let d = dups("class C { C(int x) {} C(int y) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("Duplicate constructor"), "{d:?}");
    }

    #[test]
    fn overloaded_constructor_is_ok() {
        assert!(dups("class C { C(int x) {} C(String y) {} }").is_empty());
    }

    #[test]
    fn same_name_in_different_types_is_ok() {
        // Two `f(int)` in DIFFERENT classes are unrelated.
        assert!(dups("class A { void f(int x) {} } class B { void f(int y) {} }").is_empty());
    }

    #[test]
    fn varargs_vs_scalar_is_not_duplicate() {
        assert!(dups("class C { void f(int x) {} void f(int... xs) {} }").is_empty());
    }

    #[test]
    fn no_arg_duplicate_is_flagged() {
        assert_eq!(dups("class C { int g() { return 1; } int g() { return 2; } }").len(), 1);
    }

    #[test]
    fn generic_param_exact_duplicate_is_flagged() {
        let d = dups("class C { void f(java.util.List<String> a) {} void f(java.util.List<String> b) {} }");
        assert_eq!(d.len(), 1, "{d:?}");
    }
}
