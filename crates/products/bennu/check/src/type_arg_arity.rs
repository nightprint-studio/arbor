//! Type-argument **arity**: a written `Base<A, B, …>` whose type-argument COUNT doesn't match the
//! number of type parameters `Base` actually declares — `List<A, B>` where `List` is `List<E>`, or
//! `Map<K>` where `Map` is `Map<K, V>`. A resolver-backed check: it needs the declared type-parameter
//! count, which the seam already carries (`ClassMembers::type_params`, populated from the bytecode
//! generic signature for library / JDK types and from the `<T, …>` clause for project types).
//!
//! Soundness (docs: NEVER a false positive). We flag ONLY when the base type resolves and its declared
//! type-parameter list is **non-empty** — i.e. we KNOW its exact generic arity. A base we can't resolve,
//! or one whose `type_params` came back empty, is skipped: an empty list can't be told apart from "a
//! generic type whose parameters we didn't capture", so parameterizing it is never flagged (that would
//! risk a false "type does not have type parameters"). The diamond `<>` (zero written arguments, always
//! inferred) and raw usage (no `<…>` at all → not a `generic_type` node) are never flagged. A scoped /
//! nested generic base (`Outer<X>.Inner`) is skipped too — its binary isn't reliably recoverable here.

use bennu_java::prelude::{FileSymbols, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::{Node, Parser};

use crate::resolve::type_binary;

/// Parse `source` and flag type-argument arity mismatches.
pub fn type_arg_arity_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_java::LANGUAGE.into()).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let nodes = crate::check::collect_nodes(tree.root_node());
    type_arg_arity_errors_in(&nodes, source, &symbols, resolver)
}

/// Tree-driven core: iterates the shared `nodes` + reuses the caller's `symbols`.
pub fn type_arg_arity_errors_in(
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Vec<Diagnostic> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() != "generic_type" {
            continue;
        }
        if let Some(d) = check_generic_type(n, bytes, symbols, resolver) {
            out.push(d);
        }
    }
    out
}

fn check_generic_type(
    n: Node,
    bytes: &[u8],
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
) -> Option<Diagnostic> {
    // The `<…>` node. (A `generic_type` always has one; guard anyway.)
    let mut c = n.walk();
    let args = n.children(&mut c).find(|ch| ch.kind() == "type_arguments")?;

    // The written type-argument count — named children that aren't a leading annotation
    // (`List<@NonNull String>` is ONE argument). A diamond `<>` has zero → skip (always inferred).
    let mut ac = args.walk();
    let written = args
        .named_children(&mut ac)
        .filter(|a| !matches!(a.kind(), "annotation" | "marker_annotation"))
        .count();
    if written == 0 {
        return None;
    }

    // The base type name = the text before `type_arguments`. A nested-generic base (`Outer<X>.Inner`)
    // carries a `<` here → its binary isn't reliably recoverable, so skip.
    let base_text = std::str::from_utf8(&bytes[n.start_byte()..args.start_byte()]).ok()?.trim();
    if base_text.is_empty() || base_text.contains('<') {
        return None;
    }

    // Resolve to a binary and read its DECLARED type-parameter count. Only a known, non-empty arity
    // lets us judge (see the soundness note).
    let binary = type_binary(base_text, symbols, resolver)?;
    let members = resolver.members_of(&binary)?;
    let declared = members.type_params.len();
    if declared == 0 || written == declared {
        return None;
    }

    Some(crate::check_id::CheckId::WrongTypeArgumentCount.at(
        n,
        format!("Wrong number of type arguments: {written}; required: {declared}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassMembers, Import};
    use std::collections::HashMap;
    use std::sync::Arc;

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
    fn cm(params: &[&str]) -> ClassMembers {
        ClassMembers {
            type_params: params.iter().map(|s| s.to_string()).collect(),
            superclass: Some("java/lang/Object".to_string()),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags: Default::default(),
        }
    }
    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/util/List".to_string(), cm(&["E"]));
        members.insert("java/util/Map".to_string(), cm(&["K", "V"]));
        members.insert("java/util/ArrayList".to_string(), cm(&["E"]));
        members.insert("com/acme/Box".to_string(), cm(&["T"]));
        members.insert("com/acme/Plain".to_string(), cm(&[]));
        let mut simple = HashMap::new();
        for (s, b) in [
            ("List", "java/util/List"),
            ("Map", "java/util/Map"),
            ("ArrayList", "java/util/ArrayList"),
            ("Box", "com/acme/Box"),
            ("Plain", "com/acme/Plain"),
        ] {
            simple.insert(s.to_string(), b.to_string());
        }
        MapResolver { members, simple }
    }

    fn run(src: &str) -> Vec<Diagnostic> {
        type_arg_arity_errors(src, &resolver())
    }

    #[test]
    fn too_many_args_is_flagged() {
        let d = run("class C { java.util.List<String, Integer> xs; }");
        // Note: uses the FQ form so the base resolves without an import in this bare test.
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("Wrong number of type arguments: 2; required: 1"), "{}", d[0].message);
    }

    #[test]
    fn simple_name_via_resolver_is_flagged() {
        let d = run("class C { List<String, Integer> xs; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("2; required: 1"), "{}", d[0].message);
    }

    #[test]
    fn correct_arity_is_ok() {
        assert!(run("class C { List<String> xs; }").is_empty());
        assert!(run("class C { Map<String, Integer> m; }").is_empty());
        assert!(run("class C { Box<String> b; }").is_empty());
    }

    #[test]
    fn too_few_args_is_flagged() {
        let d = run("class C { Map<String> m; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("1; required: 2"), "{}", d[0].message);
    }

    #[test]
    fn project_type_wrong_arity_is_flagged() {
        let d = run("class C { Box<String, Integer> b; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("2; required: 1"), "{}", d[0].message);
    }

    #[test]
    fn wildcard_counts_as_one_argument() {
        assert!(run("class C { List<?> xs; }").is_empty());
        assert!(run("class C { Map<?, ?> m; }").is_empty());
    }

    #[test]
    fn diamond_is_never_flagged() {
        // Empty `<>` → inferred, always legal.
        assert!(run("class C { void m() { Object o = new ArrayList<>(); } }").is_empty());
    }

    #[test]
    fn unknown_or_non_generic_base_is_skipped() {
        // Base doesn't resolve → skip (never a false positive).
        assert!(run("class C { Nonesuch<String> x; }").is_empty());
        // Base resolves but declares NO type parameters → can't tell non-generic from a capture gap → skip.
        assert!(run("class C { Plain<String> p; }").is_empty());
    }

    #[test]
    fn nested_generic_argument_is_checked_independently() {
        // Outer OK (2 args for Map), inner List<A, B> wrong (2 for List<E>).
        let d = run("class C { Map<String, List<String, Integer>> m; }");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("2; required: 1"), "{}", d[0].message);
    }
}
