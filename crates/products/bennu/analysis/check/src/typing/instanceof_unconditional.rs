//! `x instanceof T t` where `x` already IS a `T` — an unconditional pattern.
//!
//! From Java 16 an `instanceof` pattern is legal, but one that cannot fail (the expression's static type
//! is the pattern's type or a subtype of it) was rejected until Java 21: javac says `expression type T is
//! a subtype of pattern type T` at 16–20 and nothing at all from 21. So the verdict needs two things the
//! syntax tree does not give — the expression's type and the project's language level — and is made only
//! when both are known and the level is 16 to 20.
//!
//! Reported under the language-level kind, because that is how javac words it at the levels where it
//! applies (`feature.not.supported.in.source.plural`: "unconditional patterns in instanceof").

use bennu_java::prelude::{infer_node_type_cached, FileSymbols, InferCache, TypeResolver};
use bennu_proto::prelude::Diagnostic;
use tree_sitter::Node;

use crate::engine::check_id::CheckId;
use crate::support::walk::reaches;

/// Every unconditional `instanceof` pattern among `nodes`, for a project compiled at `java_major`.
pub fn instanceof_unconditional_errors_in(
    root: Node,
    nodes: &[Node],
    source: &str,
    symbols: &FileSymbols,
    resolver: &dyn TypeResolver,
    cache: &InferCache,
    java_major: Option<u32>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    if !java_major.is_some_and(|m| (16..21).contains(&m)) {
        return out;
    }
    let bytes = source.as_bytes();
    for &n in nodes {
        if n.kind() != "instanceof_expression" {
            continue;
        }
        // `x instanceof T name`: a binding is what makes it a pattern.
        let (Some(left), Some(ty), Some(_binding)) =
            (n.child_by_field_name("left"), n.child_by_field_name("right"), n.child_by_field_name("name"))
        else {
            continue;
        };
        let Ok(written) = ty.utf8_text(bytes) else { continue };
        let erased = written.split('<').next().unwrap_or(written).trim();
        let Some(pattern) = crate::support::resolve::type_binary_at(erased, n, bytes, symbols, resolver) else {
            continue;
        };
        let Some(actual) = infer_node_type_cached(&root, source, symbols, &left, resolver, cache) else { continue };
        if actual.dims != 0 || actual.binary_name.is_empty() {
            continue;
        }
        if actual.binary_name == pattern || reaches(resolver, &actual.binary_name, &pattern) {
            out.push(CheckId::FeatureRequiresNewerJava.at(
                n,
                "An `instanceof` pattern that cannot fail (the expression is already that type) needs Java 21",
            ));
        }
    }
    out
}
