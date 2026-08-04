//! What the file in front of you is, and therefore what can be written into it.
//!
//! ## Read from the buffer, not from the index
//!
//! Deliberately. The index is rebuilt off the request path, so a class you annotated `@Entity`
//! four seconds ago is not in it yet — and a toolbar that appears four seconds late reads as a
//! toolbar that is broken. Scanning the one open buffer costs a single tree-sitter parse, which
//! is what `highlights` already pays per keystroke, and it is always right.
//!
//! Guarded by [`looks_jpa_relevant`] first, so the ninety percent of a project that mentions no
//! persistence at all never reaches the parser.
//!
//! ## Offered means applicable
//!
//! A role produces actions; no role produces none. There is no disabled-button state, because a
//! greyed-out *Add query method* on a DTO teaches nothing that its absence does not.

use bennu_ext::prelude::ExtAction;

use crate::generate::LIFECYCLE_EVENTS;
use crate::index::JavaUnit;
use crate::scan::{looks_jpa_relevant, scan_java};

/// What a `.java` buffer declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRole {
    /// An `@Entity` / `@Embeddable` / `@MappedSuperclass`, with which of the three it is.
    Entity { simple: String, kind: String },
    /// A Spring Data repository, with the entity it manages.
    Repository { simple: String, entity: String },
    /// Neither — the normal answer.
    None,
}

impl FileRole {
    pub fn is_none(&self) -> bool {
        matches!(self, FileRole::None)
    }
}

/// Classify a live buffer.
///
/// Repository first: the two are mutually exclusive in practice, and asking in this order means
/// a file that somehow claimed both is treated as the one whose actions are safe to offer.
pub fn role_of(path: &str, source: &str) -> FileRole {
    if !looks_jpa_relevant(source) {
        return FileRole::None;
    }
    let Some(facts) = scan_java(path, source) else { return FileRole::None };
    let units = [JavaUnit { facts, text: source.to_string() }];

    if let Some(r) = crate::index::repositories(&units).into_iter().next() {
        return FileRole::Repository { simple: r.simple, entity: r.entity };
    }
    match crate::index::entities(&units).into_iter().next() {
        Some(e) => FileRole::Entity { simple: e.simple, kind: e.kind },
        None => FileRole::None,
    }
}

/// The toolbar for a buffer.
///
/// The grouping is the one JPA Buddy settled on and it is the right one: the things that produce
/// a **type** are plain buttons, and the things that produce a **member** are dropdowns over the
/// shape that member takes. A user reaching for "count how many match" is not reaching for a
/// different feature from "find them", so those live under one button rather than two.
pub fn actions(path: &str, source: &str) -> Vec<ExtAction> {
    match role_of(path, source) {
        FileRole::Entity { simple, kind } => entity_actions(&simple, &kind),
        FileRole::Repository { simple, entity } => repository_actions(&simple, &entity),
        FileRole::None => Vec::new(),
    }
}

fn entity_actions(simple: &str, kind: &str) -> Vec<ExtAction> {
    let mut out = vec![
        ExtAction::new("jpa.attribute", "Add attribute", "column")
            .detail(format!("Add a mapped field to {simple}")),
        ExtAction::new("jpa.lifecycle", "Add lifecycle callback", "clock").over(
            LIFECYCLE_EVENTS
                .iter()
                .map(|(event, when)| {
                    ExtAction::new(format!("jpa.lifecycle.{event}"), format!("@{event}"), "clock")
                        .detail(format!("Runs {when}"))
                })
                .collect(),
        ),
    ];
    // A `@MappedSuperclass` or an `@Embeddable` is not queryable on its own: it has no table, no
    // repository, and a named query against it would not resolve. Offering those would be
    // offering something that cannot work.
    if kind == "entity" {
        out.push(
            ExtAction::new("jpa.named-query", "Add named query", "query")
                .detail(format!("Add a @NamedQuery to {simple}")),
        );
        out.push(
            ExtAction::new("jpa.repository", "Repository", "database")
                .detail(format!("Generate a Spring Data repository for {simple}")),
        );
        out.push(
            ExtAction::new("jpa.projection", "Projection", "columns")
                .detail(format!("Generate a projection interface over {simple}")),
        );
    }
    out
}

/// The four shapes a read query comes in, plus the two a write does.
///
/// `find` twice is not a duplicate: "one row or none" and "all of them" differ in return type,
/// in what a second match means, and in which one is a bug — so they are two things to ask for.
/// The shapes worth a button. The form offers every [`crate::generate::ReturnShape`]; this is the
/// short list you reach for without opening a menu, which is why `Slice` and `Stream` are not on
/// it — they are deliberate choices made inside the form, not defaults.
const QUERY_SHAPES: &[(&str, &str, &str)] = &[
    ("single", "Single instance", "Optional<E> — one row or none"),
    ("list", "List", "List<E> — every match"),
    ("page", "Page", "Page<E> — a Pageable page, with the total count"),
    ("count", "Count", "long — how many match"),
    ("exists", "Exists", "boolean — whether any match"),
];

const MODIFY_SHAPES: &[(&str, &str, &str)] =
    &[("update", "Update", "Bulk update the matching rows"), ("delete", "Delete", "Bulk delete the matching rows")];

fn repository_actions(simple: &str, entity: &str) -> Vec<ExtAction> {
    vec![
        ExtAction::new("jpa.query", "Add query method", "search")
            .detail(format!("Build a query over {entity}"))
            .over(
                QUERY_SHAPES
                    .iter()
                    .map(|(id, label, detail)| {
                        ExtAction::new(format!("jpa.query.{id}"), *label, "search").detail(*detail)
                    })
                    .collect(),
            ),
        ExtAction::new("jpa.modify", "Add modify method", "pencil")
            .detail("Build a @Modifying bulk write")
            .over(
                MODIFY_SHAPES
                    .iter()
                    .map(|(id, label, detail)| {
                        ExtAction::new(format!("jpa.modify.{id}"), *label, "pencil").detail(*detail)
                    })
                    .collect(),
            ),
        ExtAction::new("jpa.projection", "Projection", "columns")
            .detail(format!("Generate a projection {simple} can return")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const IMPORTS: &str = "import jakarta.persistence.*; import org.springframework.data.jpa.repository.*;";

    fn java(body: &str) -> String {
        format!("package com.acme;{IMPORTS}\n{body}")
    }

    fn ids(actions: &[ExtAction]) -> Vec<&str> {
        actions.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn an_entity_buffer_offers_the_entity_authoring_actions() {
        let src = java("@Entity public class Order { @Id Long id; }");
        assert_eq!(
            role_of("/p/Order.java", &src),
            FileRole::Entity { simple: "Order".into(), kind: "entity".into() },
        );
        let a = actions("/p/Order.java", &src);
        assert_eq!(
            ids(&a),
            ["jpa.attribute", "jpa.lifecycle", "jpa.named-query", "jpa.repository", "jpa.projection"],
        );
        assert_eq!(a[1].children.len(), LIFECYCLE_EVENTS.len(), "all seven callbacks");
        assert_eq!(a[1].children[0].label, "@PrePersist");
    }

    /// A `@MappedSuperclass` has no table and no repository; offering either would be offering
    /// something that cannot work.
    #[test]
    fn a_mapped_superclass_offers_only_what_applies_to_it() {
        let src = java("@MappedSuperclass public class Auditable { @Id Long id; }");
        let a = actions("/p/Auditable.java", &src);
        assert_eq!(ids(&a), ["jpa.attribute", "jpa.lifecycle"]);
    }

    #[test]
    fn a_repository_buffer_offers_the_two_dropdowns_and_a_projection() {
        let src = java("public interface OrderRepository extends JpaRepository<Order, Long> {\n}");
        assert_eq!(
            role_of("/p/OrderRepository.java", &src),
            FileRole::Repository { simple: "OrderRepository".into(), entity: "Order".into() },
        );
        let a = actions("/p/OrderRepository.java", &src);
        assert_eq!(ids(&a), ["jpa.query", "jpa.modify", "jpa.projection"]);
        assert_eq!(
            ids(&a[0].children),
            ["jpa.query.single", "jpa.query.list", "jpa.query.page", "jpa.query.count", "jpa.query.exists"],
        );
        assert_eq!(ids(&a[1].children), ["jpa.modify.update", "jpa.modify.delete"]);
    }

    #[test]
    fn an_ordinary_class_offers_nothing_at_all() {
        let src = java("public class OrderDto { String name; }");
        assert!(role_of("/p/OrderDto.java", &src).is_none());
        assert!(actions("/p/OrderDto.java", &src).is_empty());
        // And a file that mentions no persistence never reaches the parser.
        assert!(role_of("/p/Util.java", "public class Util {}").is_none());
    }

    /// The reason this reads the buffer rather than the model: a class annotated four seconds
    /// ago is not indexed yet, and a toolbar that appears late reads as one that is broken.
    #[test]
    fn a_freshly_annotated_class_gets_its_toolbar_without_waiting_for_a_reindex() {
        let src = java("@Entity public class BrandNew { @Id Long id; }");
        assert!(!actions("/p/BrandNew.java", &src).is_empty());
    }
}
