//! **`flatMap` whose function does not return what `flatMap` wraps** — `Optional.flatMap(it -> repo.find(it))`
//! where `find` returns a plain value, not an `Optional`. javac refuses it with an inference error
//! ("incompatible types: cannot infer type-variable(s) U"); the line reads like `map`, so it is easy to write
//! and the compiler's message names a type variable rather than the mistake.
//!
//! Judged only where nothing is left to guess:
//!   * the receiver is `java.util.Optional` or `java.util.stream.Stream` (read, not assumed from the name);
//!   * the argument is a lambda — a method reference is left alone, it needs the referenced method's overloads;
//!   * a value the lambda returns is provably not that container ([`definite_mismatch`]: the same verdict the
//!     assignment and return checks use, so an `Object` that is really an erased type variable, a type variable
//!     and an uninferred value all stay silent). `null` is legal and never judged.

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeRef, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::flow::returns::collect_returns;
use crate::support::assignable::{definite_mismatch, spells_its_type};
use crate::support::nodes::simple_name;

/// A container whose `flatMap` demands a function returning the same container.
struct Container {
    binary: &'static str,
    name: &'static str,
}

const CONTAINERS: &[Container] = &[
    Container { binary: "java/util/Optional", name: "Optional" },
    Container { binary: "java/util/stream/Stream", name: "Stream" },
];

/// Parse `source` and flag `flatMap` calls whose lambda returns the wrong thing.
pub fn flat_map_return_errors(source: &str, resolver: &dyn TypeResolver) -> Vec<Diagnostic> {
    let Some(tree) = bennu_java::prelude::parse_java(source) else {
        return Vec::new();
    };
    let symbols = bennu_java::prelude::extract_symbols(source);
    let root = tree.root_node();
    let nodes = crate::engine::check::collect_nodes(root);
    flat_map_return_errors_in(root, &nodes, source, &symbols, resolver, &InferCache::new())
}

/// Tree-driven core: iterates the shared `nodes` and reuses the inference cache.
pub fn flat_map_return_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for &n in nodes {
        if n.kind() == "method_invocation" {
            check_call(n, &root, source, symbols, resolver, cache, &mut out);
        }
    }
    out
}

fn check_call(
    call: Node,
    root: &Node,
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    out: &mut Vec<Diagnostic>,
) {
    let bytes = source.as_bytes();
    let (Some(receiver), Some(name), Some(args)) =
        (call.child_by_field_name("object"), call.child_by_field_name("name"), call.child_by_field_name("arguments"))
    else {
        return;
    };
    if name.utf8_text(bytes) != Ok("flatMap") {
        return;
    }
    let mut cursor = args.walk();
    let arguments: Vec<Node> = args.named_children(&mut cursor).collect();
    let [lambda] = arguments.as_slice() else { return };
    if lambda.kind() != "lambda_expression" {
        return;
    }
    let Some(recv_type) = infer_node_type_cached(root, source, symbols, &receiver, resolver, cache) else { return };
    if recv_type.dims > 0 {
        return;
    }
    let Some(container) = CONTAINERS.iter().find(|c| c.binary == recv_type.binary_name) else { return };
    let target = TypeRef::simple(container.binary);

    for value in returned_values(*lambda) {
        let Some(found) = infer_node_type_cached(root, source, symbols, &value, resolver, cache) else { continue };
        if let Some(mismatch) = definite_mismatch(&found, &target, spells_its_type(value), resolver) {
            out.push(CheckId::FlatMapReturn.at(
                value,
                format!(
                    "`flatMap` needs a function that returns a{} `{}`, but this returns `{}` — use `map` to wrap a plain value",
                    if container.name.starts_with(['A', 'E', 'I', 'O', 'U']) { "n" } else { "" },
                    container.name,
                    simple_name(&mismatch.found),
                ),
            ));
            // One report per call is enough; the first bad value is the one to fix.
            return;
        }
    }
}

/// What the lambda hands back: its expression body, or the value of each `return` of its block body.
fn returned_values(lambda: Node) -> Vec<Node> {
    let Some(body) = lambda.child_by_field_name("body") else { return Vec::new() };
    if body.kind() != "block" {
        return vec![body];
    }
    let mut returns = Vec::new();
    collect_returns(body, &mut returns);
    returns
        .into_iter()
        .filter_map(|r| {
            let mut c = r.walk();
            let value = r.named_children(&mut c).find(|n| !n.kind().ends_with("comment"));
            value
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_java::prelude::{ClassFlags, ClassMembers, Import};
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
        fn resolve_simple_name(&self, name: &str, _: &[Import]) -> Option<String> {
            self.simple.get(name).cloned()
        }
    }

    fn class(flags: ClassFlags) -> ClassMembers {
        ClassMembers {
            superclass: Some(TypeRef::simple("java/lang/Object")),
            interfaces: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            flags,
            type_params: Vec::new(),
        }
    }

    fn resolver() -> MapResolver {
        let mut members = HashMap::new();
        members.insert("java/lang/Object".to_string(), ClassMembers { superclass: None, ..class(ClassFlags::default()) });
        members.insert("java/util/Optional".to_string(), class(ClassFlags { is_final: true, ..Default::default() }));
        members.insert("java/lang/String".to_string(), class(ClassFlags { is_final: true, ..Default::default() }));
        members.insert("java/lang/Integer".to_string(), class(ClassFlags { is_final: true, ..Default::default() }));
        members.insert(
            "java/util/stream/Stream".to_string(),
            ClassMembers { superclass: None, ..class(ClassFlags { is_interface: true, is_abstract: true, ..Default::default() }) },
        );
        members.insert("com/example/Box".to_string(), class(ClassFlags::default()));
        let simple = [
            ("Optional", "java/util/Optional"),
            ("String", "java/lang/String"),
            ("Integer", "java/lang/Integer"),
            ("Stream", "java/util/stream/Stream"),
            ("Box", "com/example/Box"),
        ]
        .into_iter()
        .map(|(s, b)| (s.to_string(), b.to_string()))
        .collect();
        MapResolver { members, simple }
    }

    fn messages(body: &str) -> Vec<String> {
        let src = format!(
            "import java.util.Optional; class C {{ void m(Optional<String> o, Stream<String> xs, String s, int n, Box b) {{ {body} }} }}"
        );
        flat_map_return_errors(&src, &resolver()).into_iter().map(|d| d.message).collect()
    }

    // ── should flag ──────────────────────────────────────────────────────────────

    #[test]
    fn an_expression_lambda_returning_a_plain_value_is_flagged() {
        let m = messages("o.flatMap(it -> s);");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("Optional") && m[0].contains("String"), "{m:?}");
    }

    #[test]
    fn a_block_lambda_returning_a_plain_value_is_flagged() {
        assert_eq!(messages("o.flatMap(it -> { return s; });").len(), 1);
    }

    #[test]
    fn a_primitive_result_is_flagged() {
        let m = messages("o.flatMap(it -> n);");
        assert_eq!(m.len(), 1, "{m:?}");
    }

    #[test]
    fn stream_flat_map_is_judged_too() {
        let m = messages("xs.flatMap(it -> s);");
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("Stream"), "{m:?}");
    }

    #[test]
    fn one_bad_return_among_good_ones_is_enough() {
        assert_eq!(messages("o.flatMap(it -> { if (n > 0) { return o; } return s; });").len(), 1);
    }

    #[test]
    fn the_code_is_flat_map_return() {
        let src = "import java.util.Optional; class C { void m(Optional<String> o, String s) { o.flatMap(it -> s); } }";
        let codes: Vec<String> = flat_map_return_errors(src, &resolver()).into_iter().map(|d| d.code).collect();
        assert_eq!(codes, ["flat-map-return"]);
    }

    // ── should NOT flag ──────────────────────────────────────────────────────────

    #[test]
    fn a_lambda_returning_an_optional_is_fine() {
        assert!(messages("o.flatMap(it -> o);").is_empty());
        assert!(messages("o.flatMap(it -> { return o; });").is_empty());
    }

    #[test]
    fn null_is_not_judged() {
        assert!(messages("o.flatMap(it -> null);").is_empty());
    }

    #[test]
    fn a_method_reference_is_left_alone() {
        assert!(messages("o.flatMap(Optional::of);").is_empty());
    }

    #[test]
    fn a_flat_map_of_another_type_is_not_judged() {
        assert!(messages("b.flatMap(it -> s);").is_empty());
    }

    #[test]
    fn an_unreadable_value_is_not_judged() {
        assert!(messages("o.flatMap(it -> mystery(it));").is_empty());
    }
}
