//! Deciding whether an annotation is the one we think it is.
//!
//! `@Service` is not a reserved word. Anyone can declare `com.acme.Service` and put it on a
//! class, and a tool that matches on the simple name alone will register a Spring bean that
//! does not exist — then navigate to it, count it in a panel, and offer it as an injection
//! candidate. Every one of those is a confident lie, which is the failure mode this crate
//! exists to avoid.
//!
//! So the origin is resolved the way the Java compiler resolves it, in the same order:
//!
//! 1. **Written qualified** (`@org.springframework.stereotype.Service`) — the source says it
//!    outright, nothing else matters.
//! 2. **A single-type import of that simple name** — `import com.acme.Service;` makes every
//!    bare `@Service` in the file *that* one. This is the decisive case and the one that
//!    catches a project's own annotation.
//! 3. **An on-demand import** of one of the expected packages
//!    (`import org.springframework.stereotype.*;`).
//! 4. **Nothing at all** — then the name can only resolve to a type in the *same package*,
//!    which by definition is not Spring's. Rejected.
//!
//! Step 4 looks strict and is simply the language: `@Service` with no import does not
//! compile unless it is declared next door.
//!
//! ## The one thing this does not see
//!
//! A **meta-annotation** — a project's `@MyService` that is itself annotated `@Service` — is
//! a real Spring stereotype and is not recognised here, because recognising it means
//! resolving the annotation's own declaration. That is an under-report: the bean is missed,
//! nothing false is claimed. The right direction when in doubt.

use crate::scan::{AnnFacts, JavaFacts};

/// An annotation this crate reasons about, and the packages it may legitimately come from.
struct Known {
    simple: &'static str,
    packages: &'static [&'static str],
}

const SPRING_STEREOTYPE: &str = "org.springframework.stereotype";
const SPRING_CONTEXT: &str = "org.springframework.context.annotation";
const SPRING_BEANS: &str = "org.springframework.beans.factory.annotation";
const SPRING_WEB: &str = "org.springframework.web.bind.annotation";
const SPRING_CONDITION: &str = "org.springframework.boot.autoconfigure.condition";
const LOMBOK: &str = "lombok";
const LOMBOK_EXPERIMENTAL: &str = "lombok.experimental";

/// The catalogue. A name absent from here is never verified — callers only ask about names
/// they act on, and asking about an unknown one is a bug, not a policy question.
const KNOWN: &[Known] = &[
    // Stereotypes.
    Known { simple: "Component", packages: &[SPRING_STEREOTYPE] },
    Known { simple: "Service", packages: &[SPRING_STEREOTYPE] },
    Known { simple: "Repository", packages: &[SPRING_STEREOTYPE] },
    Known { simple: "Controller", packages: &[SPRING_STEREOTYPE] },
    Known { simple: "RestController", packages: &[SPRING_WEB] },
    Known { simple: "ControllerAdvice", packages: &[SPRING_WEB] },
    Known { simple: "RestControllerAdvice", packages: &[SPRING_WEB] },
    Known { simple: "Configuration", packages: &[SPRING_CONTEXT] },
    // JSR-330 / Java EE, in both the javax and jakarta namespaces.
    Known { simple: "Named", packages: &["javax.inject", "jakarta.inject"] },
    Known { simple: "ManagedBean", packages: &["javax.annotation", "jakarta.annotation"] },
    Known { simple: "Inject", packages: &["javax.inject", "jakarta.inject"] },
    Known { simple: "Resource", packages: &["javax.annotation", "jakarta.annotation"] },
    // Bean definition + wiring.
    Known { simple: "Bean", packages: &[SPRING_CONTEXT] },
    Known { simple: "Primary", packages: &[SPRING_CONTEXT] },
    Known { simple: "Lazy", packages: &[SPRING_CONTEXT] },
    Known { simple: "Scope", packages: &[SPRING_CONTEXT] },
    Known { simple: "Profile", packages: &[SPRING_CONTEXT] },
    Known { simple: "DependsOn", packages: &[SPRING_CONTEXT] },
    Known { simple: "Autowired", packages: &[SPRING_BEANS] },
    Known { simple: "Qualifier", packages: &[SPRING_BEANS] },
    // `@Value` is the sharpest case in the whole table: `lombok.Value` and
    // `org.springframework.beans.factory.annotation.Value` share a simple name and mean
    // entirely different things. Without this, a `@Value` class would be read as a property
    // injection.
    Known { simple: "Value", packages: &[SPRING_BEANS] },
    // Web mappings.
    Known { simple: "RequestMapping", packages: &[SPRING_WEB] },
    Known { simple: "GetMapping", packages: &[SPRING_WEB] },
    Known { simple: "PostMapping", packages: &[SPRING_WEB] },
    Known { simple: "PutMapping", packages: &[SPRING_WEB] },
    Known { simple: "DeleteMapping", packages: &[SPRING_WEB] },
    Known { simple: "PatchMapping", packages: &[SPRING_WEB] },
    // Handler-parameter binding.
    Known { simple: "PathVariable", packages: &[SPRING_WEB] },
    Known { simple: "RequestParam", packages: &[SPRING_WEB] },
    Known { simple: "RequestBody", packages: &[SPRING_WEB] },
    Known { simple: "RequestHeader", packages: &[SPRING_WEB] },
    Known { simple: "RequestPart", packages: &[SPRING_WEB] },
    Known { simple: "CookieValue", packages: &[SPRING_WEB] },
    Known { simple: "ModelAttribute", packages: &[SPRING_WEB] },
    // Configuration-properties binding.
    Known {
        simple: "NestedConfigurationProperty",
        packages: &["org.springframework.boot.context.properties"],
    },
    Known { simple: "Name", packages: &["org.springframework.boot.context.properties.bind"] },
    // Property-bearing annotations elsewhere in Spring.
    Known { simple: "Scheduled", packages: &["org.springframework.scheduling.annotation"] },
    // The `@ConditionalOn…` family. A codebase that leans on injection to abstract lives in
    // these: whether a bean exists at all is decided here, so a bean model that can't read them
    // is describing a context that may never be built.
    Known { simple: "ConditionalOnProperty", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnBean", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnMissingBean", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnClass", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnMissingClass", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnExpression", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnWebApplication", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnNotWebApplication", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnResource", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnSingleCandidate", packages: &[SPRING_CONDITION] },
    Known { simple: "ConditionalOnJava", packages: &[SPRING_CONDITION] },
    Known { simple: "Conditional", packages: &[SPRING_CONTEXT] },
    Known {
        simple: "ConfigurationProperties",
        packages: &["org.springframework.boot.context.properties"],
    },
    // Lombok, whose generated members the bean/property model depends on.
    Known { simple: "Data", packages: &[LOMBOK] },
    Known { simple: "Setter", packages: &[LOMBOK] },
    Known { simple: "Getter", packages: &[LOMBOK] },
    Known { simple: "Builder", packages: &[LOMBOK] },
    Known { simple: "NoArgsConstructor", packages: &[LOMBOK] },
    Known { simple: "AllArgsConstructor", packages: &[LOMBOK] },
    Known { simple: "RequiredArgsConstructor", packages: &[LOMBOK] },
    Known { simple: "NonNull", packages: &[LOMBOK] },
    Known { simple: "Accessors", packages: &[LOMBOK_EXPERIMENTAL] },
];

fn packages_for(simple: &str) -> Option<&'static [&'static str]> {
    KNOWN.iter().find(|k| k.simple == simple).map(|k| k.packages)
}

/// Whether `ann` **is** the well-known annotation called `simple` — same name *and* an origin
/// that resolves to one of its packages.
///
/// An unknown `simple` (not in the catalogue) falls back to a name match, so a caller can ask
/// about an annotation whose package nobody has pinned down without silently getting `false`.
pub fn is(ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
    if ann.name != simple {
        return false;
    }
    match packages_for(simple) {
        Some(packages) => resolves_to(ann, facts, packages),
        None => true,
    }
}

/// The first of `names` that `ann` actually is, or `None`.
pub fn is_any<'a>(ann: &AnnFacts, facts: &JavaFacts, names: &[&'a str]) -> Option<&'a str> {
    names.iter().copied().find(|n| is(ann, facts, n))
}

/// Whether any annotation in `anns` is the well-known `simple`.
pub fn has(anns: &[AnnFacts], facts: &JavaFacts, simple: &str) -> bool {
    anns.iter().any(|a| is(a, facts, simple))
}

/// The well-known `simple` among `anns`, if written.
pub fn find<'a>(anns: &'a [AnnFacts], facts: &JavaFacts, simple: &str) -> Option<&'a AnnFacts> {
    anns.iter().find(|a| is(a, facts, simple))
}

/// Resolve where `ann` comes from, in the compiler's own order. See the module docs.
fn resolves_to(ann: &AnnFacts, facts: &JavaFacts, packages: &[&str]) -> bool {
    // 1. Written qualified — the source settles it.
    if let Some((pkg, _)) = ann.qualified.rsplit_once('.') {
        return packages.contains(&pkg);
    }
    // 2. A single-type import of this simple name decides, whatever it names.
    let suffix = format!(".{}", ann.name);
    if let Some(import) = facts.imports.iter().find(|i| i.ends_with(&suffix)) {
        let pkg = &import[..import.len() - suffix.len()];
        return packages.contains(&pkg);
    }
    // 3. An on-demand import of one of the expected packages.
    if facts.imports.iter().any(|i| {
        i.strip_suffix(".*").is_some_and(|pkg| packages.contains(&pkg))
    }) {
        return true;
    }
    // 4. No import: a bare name can only be a type in this file's own package, so it is the
    //    project's annotation and not the one we are looking for.
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    fn facts(src: &str) -> JavaFacts {
        scan_java("/p/T.java", src).unwrap()
    }

    /// The class-level annotations of the first type.
    fn anns(src: &str) -> (JavaFacts, Vec<AnnFacts>) {
        let f = facts(src);
        let a = f.types[0].annotations.clone();
        (f, a)
    }

    #[test]
    fn the_real_spring_service_is_recognised() {
        let (f, a) = anns("package p;\nimport org.springframework.stereotype.Service;\n@Service class C {}\n");
        assert!(is(&a[0], &f, "Service"));
    }

    #[test]
    fn someone_elses_service_is_not() {
        // The whole point: a project may declare its own, and it must not become a bean.
        let (f, a) = anns("package p;\nimport com.acme.annotations.Service;\n@Service class C {}\n");
        assert!(!is(&a[0], &f, "Service"));
    }

    #[test]
    fn a_bare_annotation_with_no_import_is_same_package_and_therefore_not_springs() {
        let (f, a) = anns("package com.acme;\n@Service class C {}\n");
        assert!(!is(&a[0], &f, "Service"), "no import means it is declared next door");
    }

    #[test]
    fn a_fully_qualified_use_needs_no_import() {
        let (f, a) = anns("package p;\n@org.springframework.stereotype.Service class C {}\n");
        assert!(is(&a[0], &f, "Service"));
        let (f2, a2) = anns("package p;\n@com.acme.Service class C {}\n");
        assert!(!is(&a2[0], &f2, "Service"));
    }

    #[test]
    fn an_on_demand_import_of_the_right_package_counts() {
        let (f, a) = anns("package p;\nimport org.springframework.stereotype.*;\n@Service class C {}\n");
        assert!(is(&a[0], &f, "Service"));
        let (f2, a2) = anns("package p;\nimport com.acme.*;\n@Service class C {}\n");
        assert!(!is(&a2[0], &f2, "Service"));
    }

    #[test]
    fn an_explicit_import_beats_an_on_demand_one() {
        // Java's own precedence: the single-type import wins, so this is com.acme's.
        let (f, a) = anns(
            "package p;\nimport org.springframework.stereotype.*;\nimport com.acme.Service;\n@Service class C {}\n",
        );
        assert!(!is(&a[0], &f, "Service"));
    }

    #[test]
    fn lombok_value_and_spring_value_are_told_apart() {
        let (f, a) = anns("package p;\nimport lombok.Value;\n@Value class C {}\n");
        assert!(!is(&a[0], &f, "Value"), "a Lombok @Value class is not a property injection");

        let src = "package p;\nimport org.springframework.beans.factory.annotation.Value;\nclass C { @Value(\"${a.b}\") String s; }\n";
        let f2 = facts(src);
        let field_ann = &f2.types[0].fields[0].annotations[0];
        assert!(is(field_ann, &f2, "Value"));
    }

    #[test]
    fn jakarta_and_javax_are_both_accepted() {
        for pkg in ["javax.inject", "jakarta.inject"] {
            let (f, a) = anns(&format!("package p;\nimport {pkg}.Named;\n@Named class C {{}}\n"));
            assert!(is(&a[0], &f, "Named"), "{pkg}");
        }
    }

    #[test]
    fn is_any_returns_which_one_matched() {
        let (f, a) = anns(
            "package p;\nimport org.springframework.stereotype.Repository;\n@Repository class C {}\n",
        );
        assert_eq!(is_any(&a[0], &f, &["Service", "Repository"]), Some("Repository"));
        assert_eq!(is_any(&a[0], &f, &["Service", "Component"]), None);
    }

    #[test]
    fn an_annotation_outside_the_catalogue_falls_back_to_its_name() {
        let (f, a) = anns("package p;\nimport com.acme.Whatever;\n@Whatever class C {}\n");
        assert!(is(&a[0], &f, "Whatever"), "nobody pinned a package for this one");
    }

    #[test]
    fn has_and_find_agree_with_is() {
        let (f, a) = anns(
            "package p;\nimport org.springframework.stereotype.Service;\n@Service(\"x\") class C {}\n",
        );
        assert!(has(&a, &f, "Service"));
        assert_eq!(find(&a, &f, "Service").unwrap().value().unwrap().value, "x");
        assert!(!has(&a, &f, "Component"));
        assert!(find(&a, &f, "Component").is_none());
    }
}
