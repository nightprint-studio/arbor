//! A stereotype on something component scanning never instantiates.
//!
//! Scanning registers a class only when it is **concrete and independent**: not an interface, not
//! abstract (unless it has `@Lookup` methods, which Spring implements), not an inner class that
//! needs an enclosing instance. On anything else `@Service` is inert — no bean, no error, and the
//! injection that expected one fails somewhere else with "No qualifying bean".
//!
//! `@Repository` is deliberately absent: a Spring Data repository is an interface **and** a bean,
//! registered by its own scanner, and `@Repository` on one is common and harmless.

use bennu_java::prelude::{annotations_of, has_modifier, named_child_of};
use bennu_proto::prelude::severity;
use tree_sitter::Node;

use super::{issue, methods, BeanIssue, Origins, CODE_INNER_CLASS, CODE_NOT_INSTANTIABLE};

const STEREOTYPES: &[&str] = &["Component", "Service", "Controller", "RestController"];

pub(super) fn check(type_decl: Node<'_>, origins: &Origins<'_>, out: &mut Vec<BeanIssue>) {
    let Some((stereotype, ann)) = origins.first_of(type_decl, STEREOTYPES) else { return };
    let name = type_decl.child_by_field_name("name").map(|n| origins.text(n)).unwrap_or_default();
    let at = (ann.start, ann.end);
    let source = origins.source;
    match type_decl.kind() {
        "interface_declaration" => {
            // An interface extending another may be a repository, and one carrying anything else —
            // `@FeignClient`, `@Mapper` — is registered by whoever reads that annotation.
            if named_child_of(type_decl, "extends_interfaces").is_some()
                || annotations_of(type_decl, source).len() > 1
            {
                return;
            }
            out.push(issue(
                CODE_NOT_INSTANTIABLE,
                severity::WARNING,
                format!(
                    "@{stereotype} on an interface registers nothing — component scanning only \
                     picks up concrete classes, so no `{name}` bean exists. Put @{stereotype} on \
                     the class that implements it"
                ),
                at,
            ));
        }
        "class_declaration" if has_modifier(type_decl, source, "abstract") => {
            if methods(type_decl).any(|m| origins.first_of(m, &["Lookup"]).is_some()) {
                return;
            }
            // Weak: legal, and very often a leftover on a base class — but the subclasses do not
            // inherit it, which is the belief this corrects.
            out.push(issue(
                CODE_NOT_INSTANTIABLE,
                severity::WEAK,
                format!(
                    "@{stereotype} on an abstract class registers nothing — component scanning \
                     skips abstract classes, and subclasses do not inherit @{stereotype}. Put it \
                     on each concrete subclass"
                ),
                at,
            ));
        }
        "class_declaration" if is_inner(type_decl, source) => {
            out.push(issue(
                CODE_INNER_CLASS,
                severity::WARNING,
                format!(
                    "@{stereotype} on an inner class registers nothing — `{name}` needs an \
                     instance of its enclosing class to exist, so component scanning skips it. \
                     Declare it static"
                ),
                at,
            ));
        }
        _ => {}
    }
}

/// A non-static member class of a class, record or enum. A class nested in an interface is
/// implicitly static, and a local or anonymous one is invisible to scanning anyway.
fn is_inner(class: Node<'_>, source: &str) -> bool {
    if has_modifier(class, source, "static") {
        return false;
    }
    let Some(body) = class.parent() else { return false };
    match body.kind() {
        "class_body" => body
            .parent()
            .is_some_and(|owner| matches!(owner.kind(), "class_declaration" | "record_declaration")),
        "enum_body_declarations" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{issues, with_code};
    use super::*;

    #[test]
    fn a_service_interface_is_reported_on_its_annotation() {
        let (src, found) = with_code("@Service public interface Orders { void go(); }", CODE_NOT_INSTANTIABLE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(&src[found[0].start..found[0].end], "@Service");
        assert_eq!(found[0].severity, "warning");
    }

    /// A Spring Data repository is a bean — just not a scanned one.
    #[test]
    fn repository_interfaces_and_anything_extending_are_left_alone() {
        let (_, found) = issues(
            "@Repository interface A extends JpaRepository<Order, Long> { }\n\
             @Repository interface B { }\n\
             @Component interface D extends Marker { }",
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_interface_carrying_another_annotation_is_left_alone() {
        let (_, found) = with_code("@Component @FeignClient(\"x\") interface A { }", CODE_NOT_INSTANTIABLE);
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn an_abstract_component_is_weak_unless_it_has_lookup_methods() {
        let (_, found) = with_code("@Service public abstract class Base { }", CODE_NOT_INSTANTIABLE);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].severity, "weak");

        let (_, found) = with_code(
            "@Component abstract class Pool { @Lookup protected abstract Object next(); }",
            CODE_NOT_INSTANTIABLE,
        );
        assert!(found.is_empty(), "Spring implements @Lookup and instantiates it: {found:?}");
    }

    #[test]
    fn an_inner_component_is_reported_and_a_static_nested_one_is_not() {
        let (_, found) = with_code(
            "class Outer {\n@Component class Inner { }\n@Component static class Nested { }\n}\n\
             interface Api { @Component class InInterface { } }",
            CODE_INNER_CLASS,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].message.contains("`Inner`"), "{}", found[0].message);
    }

    #[test]
    fn a_concrete_top_level_component_is_fine() {
        let (_, found) = issues("@Service public class Orders { }");
        assert!(found.is_empty(), "{found:?}");
    }
}
