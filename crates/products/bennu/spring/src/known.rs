//! **Spring's** annotation catalogue — which packages each name may legitimately come from.
//!
//! The rule that reads this table lives in [`bennu_facts::prelude::AnnotationTable`], because
//! resolving an annotation's origin through the file's imports is not a Spring idea: `@Entity`
//! needs the identical treatment, and copying it once per framework is how the two drift. What
//! is Spring's, and lives here, is the table itself.
//!
//! Why it matters at all: `@Service` is not a reserved word. Anyone can declare
//! `com.acme.Service` and put it on a class, and a tool that matches on the simple name alone
//! will register a Spring bean that does not exist — then navigate to it, count it in a panel,
//! and offer it as an injection candidate. Every one of those is a confident lie.
//!
//! The four functions below keep the signatures the rest of this crate has always called, so
//! the extraction cost no call site anything.

use bennu_facts::prelude::{AnnotationTable, KnownAnnotation as Known};

use crate::scan::{AnnFacts, JavaFacts};

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

/// The catalogue, behind the shared resolution rule.
const SPRING: AnnotationTable = AnnotationTable::new(KNOWN);

/// Whether `ann` **is** the well-known annotation called `simple` — same name *and* an origin
/// that resolves to one of its packages.
///
/// An unknown `simple` (not in the catalogue) falls back to a name match, so a caller can ask
/// about an annotation whose package nobody has pinned down without silently getting `false`.
pub fn is(ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
    SPRING.is(ann, facts, simple)
}

/// The first of `names` that `ann` actually is, or `None`.
pub fn is_any<'a>(ann: &AnnFacts, facts: &JavaFacts, names: &[&'a str]) -> Option<&'a str> {
    SPRING.is_any(ann, facts, names)
}

/// Whether any annotation in `anns` is the well-known `simple`.
pub fn has(anns: &[AnnFacts], facts: &JavaFacts, simple: &str) -> bool {
    SPRING.has(anns, facts, simple)
}

/// The well-known `simple` among `anns`, if written.
pub fn find<'a>(anns: &'a [AnnFacts], facts: &JavaFacts, simple: &str) -> Option<&'a AnnFacts> {
    SPRING.find(anns, facts, simple)
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

    // The resolution ORDER (qualified → single-type import → on-demand → nothing) is tested
    // where it lives, in `bennu-facts`. What is tested here is the TABLE: that each name is
    // pinned to the package it actually comes from, which is the part that goes wrong when
    // someone adds an entry from memory.

    #[test]
    fn the_real_spring_service_is_recognised_and_someone_elses_is_not() {
        let (f, a) = anns("package p;\nimport org.springframework.stereotype.Service;\n@Service class C {}\n");
        assert!(is(&a[0], &f, "Service"));
        let (f2, a2) = anns("package p;\nimport com.acme.annotations.Service;\n@Service class C {}\n");
        assert!(!is(&a2[0], &f2, "Service"), "a project may declare its own");
    }

    /// The entries most easily mis-pinned, because they are NOT in the package the rest of
    /// their family lives in. Each of these was a real judgement call.
    #[test]
    fn the_awkwardly_placed_annotations_are_pinned_where_they_actually_live() {
        let case = |import: &str, ann: &str| {
            let (f, a) = anns(&format!("package p;\nimport {import};\n@{ann} class C {{}}\n"));
            is(&a[0], &f, ann)
        };
        // A stereotype, but declared with the web annotations rather than the others.
        assert!(case("org.springframework.web.bind.annotation.RestController", "RestController"));
        // …and `@Configuration` sits with the context annotations, not the stereotypes.
        assert!(case("org.springframework.context.annotation.Configuration", "Configuration"));
        assert!(!case("org.springframework.stereotype.Configuration", "Configuration"));
        // `@Name` is under `.bind`, one package deeper than `@ConfigurationProperties`.
        assert!(case("org.springframework.boot.context.properties.bind.Name", "Name"));
        assert!(case(
            "org.springframework.boot.context.properties.ConfigurationProperties",
            "ConfigurationProperties",
        ));
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

    /// The table must have no duplicate names — a second entry for a name is unreachable, and
    /// silently so, which is how a package fix gets applied to the wrong copy.
    #[test]
    fn no_name_is_listed_twice() {
        let mut seen: Vec<&str> = Vec::new();
        for k in KNOWN {
            assert!(!seen.contains(&k.simple), "`{}` is listed twice", k.simple);
            assert!(!k.packages.is_empty(), "`{}` pins no package", k.simple);
            seen.push(k.simple);
        }
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
