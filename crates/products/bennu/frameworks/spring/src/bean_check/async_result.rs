//! An `@Async` method whose result the caller can never receive.
//!
//! The proxy returns to the caller the moment the task is submitted — before the method has run. Only
//! a `Future` (a `CompletableFuture`, a `ListenableFuture`) can carry a value back from later; for
//! anything else the caller gets `null`, and a primitive return fails in the proxy instead.
//!
//! The rule is the other way round from what it looks like: rather than "anything that is not a
//! Future", it is a closed list of types that **certainly** are not one. A project's own `Future`
//! subclass is a legal return type this cannot recognise, and silence is the safe answer there.

use bennu_proto::prelude::severity;
use tree_sitter::Node;

use super::{
    issue, method_name, methods, simple_type, BeanIssue, Origins, CODE_ASYNC_DISCARDED_RESULT,
};

/// Types that are certainly not a `Future`.
const NOT_A_FUTURE: &[&str] = &[
    "boolean", "byte", "char", "short", "int", "long", "float", "double", "Boolean", "Byte",
    "Character", "Short", "Integer", "Long", "Float", "Double", "String", "Object", "Optional",
    "List", "Set", "Map", "Collection", "Iterable", "Stream",
];

pub(super) fn check(type_decl: Node<'_>, origins: &Origins<'_>, out: &mut Vec<BeanIssue>) {
    // An interface's `@Async` describes an implementation somewhere else.
    if type_decl.kind() != "class_declaration" {
        return;
    }
    for method in methods(type_decl) {
        let Some((_, ann)) = origins.first_of(method, &["Async"]) else { continue };
        let Some(returns) = method.child_by_field_name("type").map(|t| origins.text(t)) else {
            continue;
        };
        let is_array = returns.trim_end().ends_with(']');
        if !is_array && !NOT_A_FUTURE.contains(&simple_type(returns)) {
            continue;
        }
        let name = method_name(method, origins);
        out.push(issue(
            CODE_ASYNC_DISCARDED_RESULT,
            severity::WARNING,
            format!(
                "@Async method `{name}` returns {returns}, but its caller never receives it — the \
                 call returns as soon as the task is submitted, before `{name}` has run, and hands \
                 back null. Return void, or wrap the result in a CompletableFuture"
            ),
            (ann.start, ann.end),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::with_code;
    use super::*;

    #[test]
    fn a_plain_result_is_reported_on_the_annotation() {
        let (src, found) = with_code(
            "@Service class C { @Async public String report() { return \"\"; } }",
            CODE_ASYNC_DISCARDED_RESULT,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&src[found[0].start..found[0].end], "@Async");
        assert!(found[0].message.contains("returns String"), "{}", found[0].message);
    }

    #[test]
    fn primitives_collections_and_arrays_are_reported() {
        let (_, found) = with_code(
            "class C { @Async public int a() { return 0; } \
             @Async public java.util.List<String> b() { return null; } \
             @Async public Order[] c() { return null; } }",
            CODE_ASYNC_DISCARDED_RESULT,
        );
        assert_eq!(found.len(), 3, "{found:?}");
    }

    /// `void` and every Future are how an `@Async` method is meant to be written — and a type this
    /// does not know may well be a Future.
    #[test]
    fn void_futures_and_unknown_types_are_left_alone() {
        let (_, found) = with_code(
            "class C { @Async public void a() { } \
             @Async public CompletableFuture<String> b() { return null; } \
             @Async public Future<Void> c() { return null; } \
             @Async public ListenableFuture<Order> d() { return null; } \
             @Async public ReportHandle e() { return null; } }",
            CODE_ASYNC_DISCARDED_RESULT,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// EJB's `@Asynchronous`, or a project's own `@Async`, is not Spring's.
    #[test]
    fn an_async_annotation_from_elsewhere_is_not_judged() {
        let src = "package p;\nimport com.acme.Async;\nimport org.springframework.stereotype.Service;\n\
                   @Service class C { @Async public String a() { return null; } }";
        let found = super::super::issues_in("/p/C.java", src);
        assert!(found.iter().all(|i| i.code != CODE_ASYNC_DISCARDED_RESULT), "{found:?}");
    }
}
