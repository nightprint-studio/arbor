//! Injection annotations on a **static** member.
//!
//! Spring injects into bean *instances*. `@Autowired`, `@Inject` and `@Value` on a static field or
//! method are skipped with one INFO line in a log nobody reads, so the field stays `null` and the
//! first NPE is the report. `@Resource` is stricter: the context refuses to start.

use bennu_java::prelude::has_modifier;
use bennu_proto::prelude::severity;
use tree_sitter::Node;

use super::{issue, members, BeanIssue, Origins, CODE_STATIC_INJECTION};

/// Skipped on a static member, silently.
const IGNORED_ON_STATIC: &[&str] = &["Autowired", "Value", "Inject"];

pub(super) fn check(type_decl: Node<'_>, origins: &Origins<'_>, out: &mut Vec<BeanIssue>) {
    // An interface's fields are implicitly static without saying so, and nobody injects into one.
    if !matches!(type_decl.kind(), "class_declaration" | "enum_declaration" | "record_declaration") {
        return;
    }
    for member in members(type_decl) {
        let (what, name) = match member.kind() {
            "field_declaration" => ("field", field_name(member, origins)),
            "method_declaration" => (
                "method",
                member.child_by_field_name("name").map(|n| origins.text(n)).unwrap_or_default(),
            ),
            _ => continue,
        };
        if !has_modifier(member, origins.source, "static") {
            continue;
        }
        if let Some((_, ann)) = origins.first_of(member, &["Resource"]) {
            out.push(issue(
                CODE_STATIC_INJECTION,
                severity::ERROR,
                format!(
                    "Spring refuses @Resource on a static {what} — the context fails to start \
                     with \"@Resource annotation is not supported on static {what}s\". Make \
                     `{name}` an instance {what}"
                ),
                (ann.start, ann.end),
            ));
        } else if let Some((annotation, ann)) = origins.first_of(member, IGNORED_ON_STATIC) {
            let outcome = if what == "field" { "stays unset" } else { "is never called" };
            out.push(issue(
                CODE_STATIC_INJECTION,
                severity::WARNING,
                format!(
                    "@{annotation} is ignored on a static {what} — Spring injects bean instances, \
                     never classes, so `{name}` {outcome}. Make it an instance {what}"
                ),
                (ann.start, ann.end),
            ));
        }
    }
}

/// The first declarator's name — `@Autowired static A a, b;` is one annotation on both.
fn field_name<'f>(field: Node<'_>, origins: &Origins<'f>) -> &'f str {
    field
        .child_by_field_name("declarator")
        .and_then(|d| d.child_by_field_name("name"))
        .map(|n| origins.text(n))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::super::test_support::with_code;
    use super::*;

    #[test]
    fn autowired_on_a_static_field_is_a_warning_on_the_annotation() {
        let (src, found) =
            with_code("@Service class C { @Autowired private static Clock clock; }", CODE_STATIC_INJECTION);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&src[found[0].start..found[0].end], "@Autowired");
        assert_eq!(found[0].severity, "warning");
        assert!(found[0].message.contains("`clock` stays unset"), "{}", found[0].message);
    }

    #[test]
    fn value_and_inject_on_static_members_are_warnings_too() {
        let (_, found) = with_code(
            "class C { @Value(\"${a.b}\") static String a; @Inject static void setB(Object b) { } }",
            CODE_STATIC_INJECTION,
        );
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[1].message.contains("is never called"), "{}", found[1].message);
    }

    /// `@Resource` is refused outright, not skipped.
    #[test]
    fn resource_on_a_static_field_is_an_error() {
        let (_, found) = with_code("class C { @Resource static DataSource ds; }", CODE_STATIC_INJECTION);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, "error");
    }

    /// The standard workaround — an instance setter writing a static — is correct and left alone.
    #[test]
    fn an_instance_setter_for_a_static_field_is_not_reported() {
        let (_, found) = with_code(
            "class C { private static String a; @Value(\"${a}\") void setA(String v) { a = v; } \
             @Autowired private Clock clock; }",
            CODE_STATIC_INJECTION,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// Lombok's `@Value` is not Spring's, and a project's own `@Autowired` is not either.
    #[test]
    fn a_same_named_annotation_from_elsewhere_is_not_judged() {
        let src = "package p;\nimport com.acme.Autowired;\nclass C { @Autowired static Object x; }";
        assert!(super::super::issues_in("/p/C.java", src).is_empty());
    }
}
