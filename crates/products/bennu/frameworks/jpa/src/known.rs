//! **JPA's** annotation catalogue — which packages each name may legitimately come from.
//!
//! The rule that reads this table is [`bennu_facts::prelude::AnnotationTable`]; what is JPA's,
//! and lives here, is the table. See that module for why matching on the simple name alone is
//! not good enough — `@Entity`, `@Table`, `@Column` and `@Id` are all names a project might
//! plausibly declare for itself, and `@Query` doubly so.
//!
//! ## Two namespaces, one meaning
//!
//! Every persistence annotation exists in both `javax.persistence` (JPA ≤ 2.2, Java EE) and
//! `jakarta.persistence` (JPA 3+). A legacy project and a current one differ only in which they
//! import, and both are right — hence the pairs throughout.

use bennu_facts::prelude::{AnnotationTable, KnownAnnotation as Known};

use crate::scan::{AnnFacts, JavaFacts};

/// `javax.persistence` and `jakarta.persistence`, in that order — the pair nearly every entry
/// below uses.
const PERSISTENCE: &[&str] = &["javax.persistence", "jakarta.persistence"];
/// Spring Data's repository annotations, which are a different product from JPA proper.
const DATA_REPO: &str = "org.springframework.data.repository";
const DATA_JPA_REPO: &str = "org.springframework.data.jpa.repository";
const DATA_ANNOTATION: &str = "org.springframework.data.annotation";

const KNOWN: &[Known] = &[
    // ── The entity model ────────────────────────────────────────────────────
    Known { simple: "Entity", packages: PERSISTENCE },
    Known { simple: "Table", packages: PERSISTENCE },
    Known { simple: "Id", packages: PERSISTENCE },
    Known { simple: "EmbeddedId", packages: PERSISTENCE },
    Known { simple: "IdClass", packages: PERSISTENCE },
    Known { simple: "GeneratedValue", packages: PERSISTENCE },
    Known { simple: "Column", packages: PERSISTENCE },
    Known { simple: "JoinColumn", packages: PERSISTENCE },
    Known { simple: "JoinTable", packages: PERSISTENCE },
    Known { simple: "Transient", packages: PERSISTENCE },
    Known { simple: "Enumerated", packages: PERSISTENCE },
    Known { simple: "Lob", packages: PERSISTENCE },
    Known { simple: "Version", packages: PERSISTENCE },
    Known { simple: "Embeddable", packages: PERSISTENCE },
    Known { simple: "Embedded", packages: PERSISTENCE },
    Known { simple: "MappedSuperclass", packages: PERSISTENCE },
    Known { simple: "Inheritance", packages: PERSISTENCE },
    Known { simple: "DiscriminatorColumn", packages: PERSISTENCE },
    Known { simple: "DiscriminatorValue", packages: PERSISTENCE },
    Known { simple: "SequenceGenerator", packages: PERSISTENCE },
    Known { simple: "TableGenerator", packages: PERSISTENCE },
    Known { simple: "Temporal", packages: PERSISTENCE },
    Known { simple: "Convert", packages: PERSISTENCE },
    // ── Relations ───────────────────────────────────────────────────────────
    Known { simple: "OneToOne", packages: PERSISTENCE },
    Known { simple: "OneToMany", packages: PERSISTENCE },
    Known { simple: "ManyToOne", packages: PERSISTENCE },
    Known { simple: "ManyToMany", packages: PERSISTENCE },
    Known { simple: "ElementCollection", packages: PERSISTENCE },
    Known { simple: "OrderBy", packages: PERSISTENCE },
    // ── Named queries declared on the entity ────────────────────────────────
    Known { simple: "NamedQuery", packages: PERSISTENCE },
    Known { simple: "NamedQueries", packages: PERSISTENCE },
    Known { simple: "NamedNativeQuery", packages: PERSISTENCE },
    // ── Spring Data ─────────────────────────────────────────────────────────
    // `@Query` is the one that most deserves the check: the name is generic enough that a
    // project's own is entirely plausible, and it is also the annotation this crate reads most
    // aggressively (highlighting its contents as a different language).
    Known { simple: "Query", packages: &[DATA_JPA_REPO] },
    Known { simple: "Modifying", packages: &[DATA_JPA_REPO] },
    Known { simple: "EntityGraph", packages: &[DATA_JPA_REPO] },
    Known { simple: "Param", packages: &["org.springframework.data.repository.query"] },
    Known { simple: "NoRepositoryBean", packages: &[DATA_REPO] },
    Known { simple: "RepositoryDefinition", packages: &[DATA_REPO] },
    Known { simple: "CreatedDate", packages: &[DATA_ANNOTATION] },
    Known { simple: "LastModifiedDate", packages: &[DATA_ANNOTATION] },
    // Spring's `@Transactional` shows up on repositories often enough to be worth naming; the
    // JTA one is a different annotation with the same simple name, which is exactly the case
    // this table exists for.
    Known {
        simple: "Transactional",
        packages: &["org.springframework.transaction.annotation", "javax.transaction", "jakarta.transaction"],
    },
];

const JPA: AnnotationTable = AnnotationTable::new(KNOWN);

/// Whether `ann` **is** the JPA / Spring Data annotation called `simple`.
pub fn is(ann: &AnnFacts, facts: &JavaFacts, simple: &str) -> bool {
    JPA.is(ann, facts, simple)
}

/// The first of `names` that `ann` actually is, or `None`.
pub fn is_any<'a>(ann: &AnnFacts, facts: &JavaFacts, names: &[&'a str]) -> Option<&'a str> {
    JPA.is_any(ann, facts, names)
}

/// Whether any annotation in `anns` is `simple`.
pub fn has(anns: &[AnnFacts], facts: &JavaFacts, simple: &str) -> bool {
    JPA.has(anns, facts, simple)
}

/// The annotation `simple` among `anns`, if written.
pub fn find<'a>(anns: &'a [AnnFacts], facts: &JavaFacts, simple: &str) -> Option<&'a AnnFacts> {
    JPA.find(anns, facts, simple)
}

/// The relation annotations, in the order a field is tested against them.
pub const RELATIONS: &[&str] = &["OneToOne", "OneToMany", "ManyToOne", "ManyToMany"];

/// The first of `names` written among `anns`, or the empty string.
///
/// The empty string rather than an `Option` because every caller stores the result in a field
/// that is "the relation, or none" — and a `String` that is sometimes empty models that with one
/// fewer layer than an `Option<String>` that is sometimes `Some("")`.
pub fn is_any_of(anns: &[AnnFacts], facts: &JavaFacts, names: &[&str]) -> String {
    anns.iter()
        .find_map(|a| is_any(a, facts, names))
        .map(str::to_string)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    fn anns(src: &str) -> (JavaFacts, Vec<AnnFacts>) {
        let f = scan_java("/p/T.java", src).unwrap();
        let a = f.types[0].annotations.clone();
        (f, a)
    }

    #[test]
    fn both_persistence_namespaces_are_accepted() {
        for pkg in ["javax.persistence", "jakarta.persistence"] {
            let (f, a) = anns(&format!("package p;\nimport {pkg}.Entity;\n@Entity class C {{}}\n"));
            assert!(is(&a[0], &f, "Entity"), "{pkg}");
        }
    }

    /// The reason this table exists at all: a project's own `@Entity` must not become one.
    #[test]
    fn a_projects_own_entity_annotation_is_not_jpas() {
        let (f, a) = anns("package p;\nimport com.acme.Entity;\n@Entity class C {}\n");
        assert!(!is(&a[0], &f, "Entity"));
        let (f2, a2) = anns("package com.acme;\n@Entity class C {}\n");
        assert!(!is(&a2[0], &f2, "Entity"), "no import — declared next door");
    }

    /// `@Query` is generic enough that someone's own is entirely plausible, and this crate
    /// reads its contents as a different language — so getting it wrong is loud.
    #[test]
    fn only_spring_datas_query_is_read_as_a_query() {
        let src = "package p;\nimport org.springframework.data.jpa.repository.Query;\ninterface R { @Query(\"x\") void m(); }\n";
        let f = scan_java("/p/R.java", src).unwrap();
        assert!(is(&f.types[0].methods[0].annotations[0], &f, "Query"));

        let mine = "package p;\nimport com.acme.Query;\ninterface R { @Query(\"x\") void m(); }\n";
        let f2 = scan_java("/p/R.java", mine).unwrap();
        assert!(!is(&f2.types[0].methods[0].annotations[0], &f2, "Query"));
    }

    #[test]
    fn no_name_is_listed_twice_and_every_entry_pins_a_package() {
        let mut seen: Vec<&str> = Vec::new();
        for k in KNOWN {
            assert!(!seen.contains(&k.simple), "`{}` is listed twice", k.simple);
            assert!(!k.packages.is_empty(), "`{}` pins no package", k.simple);
            seen.push(k.simple);
        }
    }

    #[test]
    fn every_relation_name_is_in_the_table() {
        for r in RELATIONS {
            assert!(KNOWN.iter().any(|k| &k.simple == r), "{r}");
        }
    }
}
