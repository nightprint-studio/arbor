//! The Java scan, and what makes a file worth running it on **for JPA**.
//!
//! The scan itself is [`bennu_facts`]; what is JPA's, and lives here, is the relevance
//! pre-filter. The re-exports mean `crate::scan::…` reads the same inside this crate as it does
//! inside `bennu-spring`, which is the point of having extracted it.

pub use bennu_facts::prelude::{
    mentions_any, scan_java, AnnFacts, AnnString, FieldFacts, JavaFacts, MethodFacts, ParamFacts,
    TypeFacts,
};

/// Substrings that mean a file is worth parsing for JPA facts.
///
/// Deliberately over-inclusive — a false hit costs one parse, a false miss costs a feature
/// silently not working on a file. `Repository` and `Dao` are in here without an `@` because a
/// Spring Data repository is an *interface with no annotation at all*: `interface OrderRepository
/// extends JpaRepository<Order, Long>` mentions nothing else this list could match, and it is
/// exactly the file the whole crate is about.
pub const JPA_MARKERS: &[&str] = &[
    "@Entity",
    "@Table",
    "@Column",
    "@Id",
    "@Embeddable",
    "@MappedSuperclass",
    "@OneToMany",
    "@ManyToOne",
    "@OneToOne",
    "@ManyToMany",
    "@Query",
    "@NamedQuery",
    "@Modifying",
    "persistence",  // javax.persistence / jakarta.persistence / persistence.xml
    "Repository",   // the un-annotated Spring Data interface
    "Dao",
    "hibernate",
    "EntityManager",
];

/// Whether `source` mentions anything JPA-shaped at all.
pub fn looks_jpa_relevant(source: &str) -> bool {
    mentions_any(source, JPA_MARKERS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefilter_admits_the_files_that_matter() {
        assert!(looks_jpa_relevant("@Entity public class Order {}"));
        assert!(looks_jpa_relevant("import jakarta.persistence.Entity;"));
        // The case the `@`-less markers exist for: an interface with no annotation on it.
        assert!(looks_jpa_relevant(
            "public interface OrderRepository extends JpaRepository<Order, Long> {}"
        ));
        assert!(!looks_jpa_relevant("public class PlainOldJava { int x; }"));
    }
}
