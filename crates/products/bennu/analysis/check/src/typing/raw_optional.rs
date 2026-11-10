//! Raw `Optional` in declaration positions: `Optional foo()` instead of `Optional<Foo>`.
//!
//! A method (or field, or local variable) typed as raw `Optional` — without a type argument —
//! causes javac to report "incompatible types: cannot infer type-variable(s) U" at EVERY call
//! site that chains `.flatMap(…)` or `.map(…)` on the returned value. The error appears on the
//! call site, not here, so the root cause is invisible without this lint.
//!
//! Requires the resolver to confirm that the simple name "Optional" actually binds to
//! `java.util.Optional`; a project class also named `Optional` and declaring no type parameters
//! would not resolve to `java/util/Optional` and is therefore left alone.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::resolve::type_binary_at;

/// Flag every method/field/local declaration whose type is raw `java.util.Optional` (no `<T>`).
pub fn raw_optional_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        // A `generic_type` node (`Optional<Foo>`) is never a `type_identifier`; only the raw
        // form `Optional` (no `<…>`) produces a bare `type_identifier`.
        if n.kind() != "type_identifier" {
            continue;
        }
        let Ok(text) = n.utf8_text(bytes) else { continue };
        // Quick pre-filter — avoids a resolver round-trip for every identifier in the file.
        if text != "Optional" {
            continue;
        }
        // The node must be in a declaration type position: method return, field, local, or parameter.
        let Some(parent) = n.parent() else { continue };
        if !matches!(
            parent.kind(),
            "method_declaration" | "field_declaration" | "local_variable_declaration" | "formal_parameter"
        ) {
            continue;
        }
        // Confirm it is specifically the `type` child — not the name or some other position.
        let Some(type_child) = parent.child_by_field_name("type") else { continue };
        if type_child.id() != n.id() {
            continue;
        }
        // Resolve to the binary to guard against a local class also named `Optional`.
        let Some(binary) = type_binary_at(text, n, bytes, symbols, resolver) else { continue };
        if binary != "java/util/Optional" {
            continue;
        }
        out.push(CheckId::RawOptional.at(
            n,
            "Raw `Optional` — add a type argument (`Optional<Foo>`) to let callers chain \
             `.map()` / `.flatMap()` safely; without one, javac cannot infer the type \
             variable and reports \"cannot infer type-variable(s)\" at every call site",
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import, TypeResolver};
    use std::collections::HashMap;
    use std::sync::Arc;

    struct Resolver(HashMap<String, ClassMembers>, HashMap<String, String>);
    impl TypeResolver for Resolver {
        fn members_of(&self, b: &str) -> Option<Arc<ClassMembers>> {
            self.0.get(b).cloned().map(Arc::new)
        }
        fn resolve_simple_name(&self, n: &str, _: &[Import]) -> Option<String> {
            self.1.get(n).cloned()
        }
    }

    fn cm(params: &[&str]) -> ClassMembers {
        ClassMembers {
            type_params: params.iter().map(|s| s.to_string()).collect(),
            superclass: Some(bennu_java::prelude::TypeRef::simple("java/lang/Object")),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: ClassFlags::default(),
        }
    }

    fn resolver() -> Resolver {
        let mut m = HashMap::new();
        m.insert("java/util/Optional".to_string(), cm(&["T"]));
        let mut s = HashMap::new();
        s.insert("Optional".to_string(), "java/util/Optional".to_string());
        Resolver(m, s)
    }

    fn run(src: &str) -> Vec<String> {
        let Some(tree) = bennu_java::prelude::parse_java(src) else { return Vec::new() };
        let symbols = bennu_java::prelude::extract_symbols(src);
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        raw_optional_errors_in(&nodes, src, &symbols, &resolver())
            .into_iter()
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn raw_optional_method_return_is_flagged() {
        let d = run("class C { Optional lookup(String k) { return Optional.empty(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].contains("type argument"), "{}", d[0]);
    }

    #[test]
    fn raw_optional_field_is_flagged() {
        let d = run("class C { Optional cached; }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn raw_optional_local_variable_is_flagged() {
        let d = run("class C { void m() { Optional x = Optional.empty(); } }");
        assert_eq!(d.len(), 1, "{d:?}");
    }

    #[test]
    fn parameterized_optional_is_not_flagged() {
        assert!(run("class C { Optional<String> lookup(String k) { return Optional.empty(); } }").is_empty());
    }

    #[test]
    fn raw_optional_parameter_is_flagged() {
        assert_eq!(run("class C { void m(Optional x) { } }").len(), 1);
    }

    #[test]
    fn optional_in_a_cast_is_not_flagged() {
        // The declaration is parameterized; only the cast is raw, and a cast is not a declaration position.
        assert!(run("class C { void m(Object o) { Optional<?> x = (Optional) o; } }").is_empty());
    }

    #[test]
    fn unresolved_optional_is_not_flagged() {
        // A bare resolver that doesn't know about Optional at all → no binding → skip.
        let resolver = Resolver(HashMap::new(), HashMap::new());
        let Some(tree) = bennu_java::prelude::parse_java("class C { Optional lookup(String k) {} }") else { return };
        let symbols = bennu_java::prelude::extract_symbols("class C { Optional lookup(String k) {} }");
        let nodes = crate::engine::check::collect_nodes(tree.root_node());
        assert!(raw_optional_errors_in(&nodes, "class C { Optional lookup(String k) {} }", &symbols, &resolver).is_empty());
    }
}
