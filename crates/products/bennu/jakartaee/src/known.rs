//! **Jakarta EE's** annotation catalogue — which packages each name may legitimately come from.
//!
//! The resolution rule is [`bennu_facts::prelude::AnnotationTable`]'s; the table is this crate's. Two
//! names need more than a table row, and they are the reason this module is more than a list:
//!
//! - **`@Singleton`** is `javax.inject.Singleton` (a CDI pseudo-scope) *and* `javax.ejb.Singleton` (a
//!   session bean with a container-managed lock). Same simple name, unrelated meaning — and only the
//!   second is subject to the EJB class rules. A table keyed by simple name holds one of them, so
//!   neither is in it: both are resolved against their own package list directly.
//! - **`@Produces`** is CDI's producer annotation and JAX-RS's media-type annotation. Only the CDI
//!   packages are pinned, so a resource method's `@Produces("application/json")` is never read as a
//!   bean.

use bennu_facts::prelude::{resolves_to, AnnFacts, AnnotationTable, JavaFacts, KnownAnnotation as Known};

pub const CONTEXT: &[&str] = &["jakarta.enterprise.context", "javax.enterprise.context"];
pub const CDI_INJECT: &[&str] = &["jakarta.enterprise.inject", "javax.enterprise.inject"];
pub const INJECT: &[&str] = &["jakarta.inject", "javax.inject"];
pub const EJB: &[&str] = &["jakarta.ejb", "javax.ejb"];
const INTERCEPTOR: &[&str] = &["jakarta.interceptor", "javax.interceptor"];
const DECORATOR: &[&str] = &["jakarta.decorator", "javax.decorator"];
const ANNOTATION: &[&str] = &["jakarta.annotation", "javax.annotation"];
const SERVLET: &[&str] = &["jakarta.servlet.annotation", "javax.servlet.annotation"];

/// The catalogue. `Singleton` is deliberately absent — see the module docs.
const KNOWN: &[Known] = &[
    // Scopes.
    Known { simple: "ApplicationScoped", packages: CONTEXT },
    Known { simple: "RequestScoped", packages: CONTEXT },
    Known { simple: "SessionScoped", packages: CONTEXT },
    Known { simple: "ConversationScoped", packages: CONTEXT },
    Known { simple: "Dependent", packages: CONTEXT },
    Known { simple: "NormalScope", packages: CONTEXT },
    // CDI's own annotations.
    Known { simple: "Model", packages: CDI_INJECT },
    Known { simple: "Produces", packages: CDI_INJECT },
    Known { simple: "Alternative", packages: CDI_INJECT },
    Known { simple: "Specializes", packages: CDI_INJECT },
    Known { simple: "Vetoed", packages: CDI_INJECT },
    Known { simple: "Typed", packages: CDI_INJECT },
    Known { simple: "Stereotype", packages: CDI_INJECT },
    Known { simple: "Default", packages: CDI_INJECT },
    Known { simple: "Any", packages: CDI_INJECT },
    Known { simple: "New", packages: CDI_INJECT },
    // JSR-330.
    Known { simple: "Inject", packages: INJECT },
    Known { simple: "Named", packages: INJECT },
    Known { simple: "Qualifier", packages: INJECT },
    // EJB.
    Known { simple: "Stateless", packages: EJB },
    Known { simple: "Stateful", packages: EJB },
    Known { simple: "MessageDriven", packages: EJB },
    Known { simple: "EJB", packages: EJB },
    // Interceptors and decorators — beans, but never injected.
    Known { simple: "Interceptor", packages: INTERCEPTOR },
    Known { simple: "Decorator", packages: DECORATOR },
    Known { simple: "Priority", packages: ANNOTATION },
    // Servlets.
    Known { simple: "WebServlet", packages: SERVLET },
    Known { simple: "WebFilter", packages: SERVLET },
    Known { simple: "WebListener", packages: SERVLET },
];

const TABLE: AnnotationTable = AnnotationTable::new(KNOWN);

/// The normal scopes — the ones whose beans are reached through a client proxy.
pub const NORMAL_SCOPES: &[&str] =
    &["ApplicationScoped", "RequestScoped", "SessionScoped", "ConversationScoped"];

/// The session-bean and message-driven annotations, `Singleton` resolved against `javax.ejb` only.
pub const EJB_KINDS: &[&str] = &["Stateless", "Stateful", "Singleton", "MessageDriven"];

/// Whether `ann` is the catalogued annotation `simple`. Unlike the shared table, a name this crate
/// never pinned answers `false`: every question asked here is about one it did pin.
pub fn is(ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
    match simple {
        "Singleton" => is_ejb_singleton(ann, facts),
        _ => TABLE.packages_for(simple).is_some() && TABLE.is(ann, facts, simple),
    }
}

/// The annotation `simple` among `anns`, if written.
pub fn find<'a>(anns: &'a [AnnFacts], facts: &JavaFacts, simple: &str) -> Option<&'a AnnFacts> {
    anns.iter().find(|a| is(a, facts, simple))
}

pub fn has(anns: &[AnnFacts], facts: &JavaFacts, simple: &str) -> bool {
    find(anns, facts, simple).is_some()
}

/// `javax.ejb.Singleton` / `jakarta.ejb.Singleton` — the session bean.
pub fn is_ejb_singleton(ann: &AnnFacts, facts: &JavaFacts) -> bool {
    ann.name == "Singleton" && resolves_to(ann, facts, EJB)
}

/// `javax.inject.Singleton` / `jakarta.inject.Singleton` — the pseudo-scope.
pub fn is_inject_singleton(ann: &AnnFacts, facts: &JavaFacts) -> bool {
    ann.name == "Singleton" && resolves_to(ann, facts, INJECT)
}

/// The EJB kind a declaration carries (`"Stateless"`, …), if any.
pub fn ejb_kind(anns: &[AnnFacts], facts: &JavaFacts) -> Option<&'static str> {
    EJB_KINDS.iter().copied().find(|k| has(anns, facts, k))
}

/// The scope annotation a declaration carries, among the normal scopes and `@Dependent`.
pub fn scope_of(anns: &[AnnFacts], facts: &JavaFacts) -> Option<&'static str> {
    NORMAL_SCOPES.iter().chain(["Dependent"].iter()).copied().find(|s| has(anns, facts, s))
}

/// Whether `ann` is one of the platform annotations with a meaning of its own (catalogued, or either
/// `@Singleton`) — as opposed to something the reader cannot classify.
pub fn is_catalogued(ann: &AnnFacts, facts: &JavaFacts) -> bool {
    if ann.name == "Singleton" {
        return is_ejb_singleton(ann, facts) || is_inject_singleton(ann, facts);
    }
    TABLE.packages_for(&ann.name).is_some() && TABLE.is(ann, facts, &ann.name)
}

/// One line on what an annotation means, for the hover card.
pub fn meaning(simple: &str) -> Option<&'static str> {
    Some(match simple {
        "ApplicationScoped" => "One instance for the whole application, shared by every client through a proxy.",
        "RequestScoped" => "One instance per request; each request sees its own, through a proxy.",
        "SessionScoped" => "One instance per HTTP session, reached through a proxy; must be Serializable to passivate.",
        "ConversationScoped" => "One instance per JSF conversation, which spans requests until it is ended explicitly.",
        "Dependent" => "A new instance for every injection point; lives as long as the bean it is injected into.",
        "Stateless" => "A pooled session bean: any instance may serve any call, so it keeps no client state.",
        "Stateful" => "A session bean with one instance per client, keeping state between calls.",
        "Singleton" => "One session bean instance for the application, with container-managed concurrency.",
        "MessageDriven" => "A bean the container calls for each message on a queue or topic; never injected.",
        _ => return None,
    })
}

/// Whether a dotted name is in the platform's own namespaces — where no qualifier a project could
/// write lives, apart from the ones this crate already knows by name.
pub fn is_platform_name(dotted: &str) -> bool {
    ["jakarta.", "javax.", "java."].iter().any(|p| dotted.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_facts::prelude::scan_java;

    fn first_class_ann(src: &str) -> (JavaFacts, AnnFacts) {
        let f = scan_java("/p/T.java", src).unwrap();
        let a = f.types[0].annotations[0].clone();
        (f, a)
    }

    #[test]
    fn the_two_singletons_are_told_apart() {
        let (f, a) = first_class_ann("package p;\nimport javax.ejb.Singleton;\n@Singleton class C {}\n");
        assert!(is(&a, &f, "Singleton"));
        assert!(!is_inject_singleton(&a, &f));
        let (f, a) = first_class_ann("package p;\nimport javax.inject.Singleton;\n@Singleton class C {}\n");
        assert!(!is(&a, &f, "Singleton"), "the pseudo-scope is not a session bean");
        assert!(is_inject_singleton(&a, &f));
        assert_eq!(ejb_kind(&f.types[0].annotations, &f), None);
    }

    #[test]
    fn jax_rs_produces_is_not_a_producer() {
        let (f, a) = first_class_ann("package p;\nimport javax.ws.rs.Produces;\n@Produces(\"text/plain\") class C {}\n");
        assert!(!is(&a, &f, "Produces"));
        let (f, a) = first_class_ann("package p;\nimport jakarta.enterprise.inject.Produces;\n@Produces class C {}\n");
        assert!(is(&a, &f, "Produces"));
    }

    #[test]
    fn javax_and_jakarta_are_both_accepted() {
        for pkg in ["javax.enterprise.context", "jakarta.enterprise.context"] {
            let (f, a) = first_class_ann(&format!("package p;\nimport {pkg}.ApplicationScoped;\n@ApplicationScoped class C {{}}\n"));
            assert!(is(&a, &f, "ApplicationScoped"), "{pkg}");
        }
    }

    #[test]
    fn an_uncatalogued_name_is_never_a_match() {
        let (f, a) = first_class_ann("package p;\nimport com.acme.Whatever;\n@Whatever class C {}\n");
        assert!(!is(&a, &f, "Whatever"));
        assert!(!is_catalogued(&a, &f));
    }

    #[test]
    fn no_name_is_listed_twice() {
        let mut seen: Vec<&str> = Vec::new();
        for k in KNOWN {
            assert!(!seen.contains(&k.simple), "`{}` is listed twice", k.simple);
            seen.push(k.simple);
        }
    }
}
