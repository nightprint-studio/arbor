//! Duplicate-signature diagnostics — two methods or two constructors in the same type with the
//! **same name and the same parameter types**. Pure AST: signatures are compared by their written
//! parameter-type text, so `f(int)` twice is flagged while `f(int)` / `f(String)` (a legal overload)
//! is not.
//!
//! Conservative: comparison is by source text (generics kept), so it only reports an *exact*
//! duplicate — never a subtle erasure clash (`f(List<String>)` vs `f(List<Integer>)`), which stays
//! silent rather than risk a false positive.
//!
//! ## A method's own type parameters are part of its signature
//!
//! Comparing parameter text alone is not enough when the method declares its own type variables,
//! because the same letter can name two different types:
//!
//! ```text
//! static <X extends Collection> boolean many(X c)   // erases to many(Collection)
//! static <X extends Map>        boolean many(X c)   // erases to many(Map)
//! ```
//!
//! Both parameter lists read `X`, so a text comparison called them the same method — they are a
//! legal overload, and the compiler accepts them.
//!
//! The fix is to compare the parameters **after substituting each of the member's own type
//! variables by what it erases to** — its bound, or `Object` when it has none. That single rule
//! answers both directions of the question, because both are questions about erasure:
//!
//! ```text
//! <X extends Collection> many(X c)  →  many(Collection)   distinct: a legal overload
//! <X extends Map>        many(X c)  →  many(Map)
//!
//! <T> void f(T a)                   →  f(Object)          identical: a real duplicate
//! <U> void f(U b)                   →  f(Object)
//! ```
//!
//! Substituting rather than keying on the bounds beside the parameters, because the parameter's
//! written text carries the variable's **name**: `f(T)` and `f(U)` read as different signatures
//! however the bounds are compared, while the compiler sees `f(Object)` twice and rejects the pair.

use std::collections::HashMap;

use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

/// Flag duplicate method / constructor signatures within each type.
pub fn duplicate_signatures(source: &str) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
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
    // Key: (enclosing body node id, member kind + name, parameter types with the member's own type
    // variables erased). First-seen node kept; a second insertion is a duplicate.
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
        let params = param_types(n, bytes, &type_var_erasures(n, bytes));
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

/// The written parameter types of a method/constructor, whitespace-normalised and with the
/// member's own type variables replaced by what they erase to. A varargs `T...` keeps its `...` so
/// `f(T...)` and `f(T)` stay distinct.
fn param_types(member: Node, bytes: &[u8], vars: &HashMap<String, String>) -> Vec<String> {
    let Some(params) = member.child_by_field_name("parameters") else { return Vec::new() };
    let mut out = Vec::new();
    let mut c = params.walk();
    let erase = |t: &str| substitute_type_vars(&normalize(t), vars);
    for p in params.named_children(&mut c) {
        match p.kind() {
            "formal_parameter" => {
                if let Some(t) = p.child_by_field_name("type").and_then(|t| t.utf8_text(bytes).ok()) {
                    out.push(erase(t));
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
                            out.push(format!("{}...", erase(t)));
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

/// The member's own type variables, mapped to what each **erases to** — its bound, or `Object`
/// when it has none. Empty when the member declares no type parameters, which is the common case
/// and costs nothing.
///
/// A multi-bound `<T extends A & B>` keeps its whole bound text rather than erasing to `A` the way
/// javac does, so it stays distinct from `<T extends A>`. That under-reports, which is this
/// check's standing preference over risking a false positive.
fn type_var_erasures(member: Node, bytes: &[u8]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(tps) = member.child_by_field_name("type_parameters") else { return out };
    let mut c = tps.walk();
    for tp in tps.named_children(&mut c) {
        if tp.kind() != "type_parameter" {
            continue;
        }
        // `type_parameter` is `[annotations] <name> [type_bound]` — no field names, so the
        // variable is its FIRST name child and the bound its optional `type_bound`. The grammar
        // aliases that name to `type_identifier` (which is how the rest of the crate reads one);
        // `identifier` is accepted beside it so a grammar bump cannot silently empty this map and
        // turn every substitution below into the identity.
        let mut name: Option<String> = None;
        let mut bound = String::new();
        let mut bc = tp.walk();
        for ch in tp.named_children(&mut bc) {
            match ch.kind() {
                "type_identifier" | "identifier" if name.is_none() => {
                    name = ch.utf8_text(bytes).ok().map(str::to_string);
                }
                "type_bound" => {
                    // The node's text is `extends X` — the keyword is not part of the type.
                    if let Ok(t) = ch.utf8_text(bytes) {
                        let t = t.trim_start();
                        bound = normalize(t.strip_prefix("extends").unwrap_or(t));
                    }
                }
                _ => {}
            }
        }
        if let Some(name) = name {
            // Unbounded erases to `Object` — which is precisely what two unbounded variables share
            // and why `<T> f(T)` and `<U> f(U)` are one method, not two.
            out.insert(name, if bound.is_empty() { "Object".to_string() } else { bound });
        }
    }
    out
}

/// Replace every whole-word occurrence of one of `vars`' names with what it erases to.
///
/// Whole words, and never a segment that follows a `.`: a qualified `java.util.Map` must survive a
/// method that happens to call one of its variables `Map`.
fn substitute_type_vars(text: &str, vars: &HashMap<String, String>) -> String {
    if vars.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let mut qualified = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '$' {
            word.push(ch);
            continue;
        }
        flush_word(&mut out, &mut word, vars, qualified);
        qualified = ch == '.';
        out.push(ch);
    }
    flush_word(&mut out, &mut word, vars, qualified);
    out
}

fn flush_word(out: &mut String, word: &mut String, vars: &HashMap<String, String>, qualified: bool) {
    if word.is_empty() {
        return;
    }
    match vars.get(word.as_str()) {
        Some(erasure) if !qualified => out.push_str(erasure),
        _ => out.push_str(word),
    }
    word.clear();
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

    // ── a method's own type parameters ───────────────────────────────────────────

    /// The reported bug: two overloads distinguished only by their type variable's BOUND both read
    /// `(X collection)` in source, so a text comparison called them the same method. They erase to
    /// `many(Collection)` and `many(Map)` — a legal overload the compiler accepts.
    #[test]
    fn different_type_parameter_bounds_are_a_legal_overload() {
        let d = dups(
            "class C {\n\
               static <X extends java.util.Collection> boolean many(X c) { return true; }\n\
               static <X extends java.util.Map> boolean many(X c) { return true; }\n\
             }",
        );
        assert!(d.is_empty(), "distinct bounds are distinct signatures: {d:?}");
    }

    /// The name of the variable must not matter — two unbounded variables ARE the same type after
    /// erasure, so this pair is a real duplicate and has to stay flagged.
    #[test]
    fn unbounded_type_variables_of_different_names_still_collide() {
        let d = dups("class C { <T> void f(T a) {} <U> void f(U b) {} }");
        assert_eq!(d.len(), 1, "both erase to f(Object): {d:?}");
    }

    /// The substitution reaches inside a type argument — `List<T>` and `List<U>` are one
    /// parameter list, and a nested variable is where the name would otherwise survive.
    #[test]
    fn a_type_variable_inside_a_type_argument_is_erased_too() {
        let d = dups(
            "class C {\n\
               <T> void f(java.util.List<T> a) {}\n\
               <U> void f(java.util.List<U> b) {}\n\
             }",
        );
        assert_eq!(d.len(), 1, "both erase to f(List<Object>): {d:?}");
    }

    /// Whole words, and never a segment after a `.` — a method whose variable happens to be
    /// called `Map` must not rewrite `java.util.Map` out of another parameter.
    #[test]
    fn a_variable_named_like_a_package_segment_leaves_qualified_names_alone() {
        let d = dups(
            "class C {\n\
               <Map> void f(Map a, java.util.Map b) {}\n\
               <Map> void f(java.util.Map a, Map b) {}\n\
             }",
        );
        assert!(d.is_empty(), "f(Object,Map) is not f(Map,Object): {d:?}");
    }

    /// The same bound spelled the same way is still a duplicate.
    #[test]
    fn identical_bounds_are_still_a_duplicate() {
        let d = dups(
            "class C {\n\
               <X extends java.util.Map> void f(X a) {}\n\
               <X extends java.util.Map> void f(X b) {}\n\
             }",
        );
        assert_eq!(d.len(), 1, "{d:?}");
    }

    /// A generic method next to a non-generic one of the same parameter text: distinct, since one
    /// declares a variable and the other names a real class.
    #[test]
    fn a_generic_and_a_non_generic_overload_are_distinct() {
        let d = dups("class C { <X extends Number> void f(X a) {} void f(X b) {} }");
        assert!(d.is_empty(), "{d:?}");
    }

    /// Multi-bound and arity differences in the type-parameter list are respected.
    #[test]
    fn multi_bound_and_arity_differences_are_respected() {
        let d = dups(
            "class C {\n\
               <T extends A & B> void f(T a) {}\n\
               <T extends A> void f(T b) {}\n\
             }",
        );
        assert!(d.is_empty(), "`A & B` is not `A`: {d:?}");
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
