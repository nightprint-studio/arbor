//! Building the model: entities out of annotated classes, repositories out of interfaces.
//!
//! Both halves are deliberately literal — they record what is written, and every judgement
//! about what it *means* is made later, where it can be gated. The one thing worth knowing
//! before reading: a Spring Data repository usually carries **no annotation at all**. It is
//! recognised by what it extends, which is why the relevance pre-filter has to admit files on
//! the strength of a bare `Repository` in their text.

use crate::known;
use crate::model::{
    line_at, simple_name, strip_generics, type_argument, Entity, EntityField, MethodParam,
    QueryDef, RepoMethod, Repository,
};
use crate::scan::{AnnFacts, JavaFacts, TypeFacts};

/// One scanned Java file, with the text the spans point into.
#[derive(Debug, Clone)]
pub struct JavaUnit {
    pub facts: JavaFacts,
    pub text: String,
}

/// The Spring Data base interfaces a repository is recognised by.
///
/// Matched on the simple name, and that is on purpose: the check that the name is really Spring
/// Data's would need the interface's own declaration, which is in a jar. Being generous here
/// costs an extra row in a panel; being strict would lose every repository in a project that
/// wraps them in its own base interface.
const REPOSITORY_BASES: &[&str] = &[
    "JpaRepository",
    "CrudRepository",
    "ListCrudRepository",
    "PagingAndSortingRepository",
    "ListPagingAndSortingRepository",
    "JpaSpecificationExecutor",
    "QuerydslPredicateExecutor",
    "Repository",
];

/// Every `@Entity` / `@Embeddable` / `@MappedSuperclass` in the scan.
pub fn entities(units: &[JavaUnit]) -> Vec<Entity> {
    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            let Some(kind) = entity_kind(t, &u.facts) else { continue };
            let ann = known::find(&t.annotations, &u.facts, "Entity");
            let entity_name = ann
                .and_then(|a| a.strings_for("name").next())
                .map(|s| s.value.clone())
                .unwrap_or_else(|| t.name.clone());
            out.push(Entity {
                fqcn: t.fqcn.clone(),
                simple: t.name.clone(),
                entity_name,
                table: known::find(&t.annotations, &u.facts, "Table")
                    .and_then(|a| a.strings_for("name").next().or_else(|| a.value()))
                    .map(|s| s.value.clone())
                    .unwrap_or_default(),
                kind: kind.to_string(),
                extends: strip_generics(&t.extends),
                fields: entity_fields(t, &u.facts, &u.text),
                file: u.facts.file.clone(),
                offset: t.name_offset,
                line: line_at(&u.text, t.name_offset),
            });
        }
    }
    out
}

fn entity_kind(t: &TypeFacts, facts: &JavaFacts) -> Option<&'static str> {
    if known::has(&t.annotations, facts, "Entity") {
        Some("entity")
    } else if known::has(&t.annotations, facts, "Embeddable") {
        Some("embeddable")
    } else if known::has(&t.annotations, facts, "MappedSuperclass") {
        Some("mapped-superclass")
    } else {
        None
    }
}

fn entity_fields(t: &TypeFacts, facts: &JavaFacts, text: &str) -> Vec<EntityField> {
    t.fields
        .iter()
        .filter(|f| !f.is_static)
        .map(|f| {
            let relation = known::is_any_of(&f.annotations, facts, known::RELATIONS);
            // A to-many relation's target is inside the collection, a to-one's is the field type.
            let target = match relation.as_str() {
                "" => String::new(),
                _ => type_argument(&f.type_text, 0)
                    .unwrap_or_else(|| strip_generics(&f.type_text)),
            };
            EntityField {
                name: f.name.clone(),
                type_text: f.type_text.clone(),
                column: known::find(&f.annotations, facts, "Column")
                    .and_then(|a| a.strings_for("name").next().or_else(|| a.value()))
                    .map(|s| s.value.clone())
                    .unwrap_or_default(),
                is_id: known::has(&f.annotations, facts, "Id")
                    || known::has(&f.annotations, facts, "EmbeddedId"),
                relation,
                target,
                transient: known::has(&f.annotations, facts, "Transient"),
                offset: f.name_offset,
                line: line_at(text, f.name_offset),
            }
        })
        .collect()
}

/// Every Spring Data repository interface in the scan.
pub fn repositories(units: &[JavaUnit]) -> Vec<Repository> {
    let mut out = Vec::new();
    for u in units {
        for t in &u.facts.types {
            // An interface's base list lands in `implements` or in `extends` depending on how it
            // is written; both are searched rather than guessing which.
            let bases: Vec<&String> = std::iter::once(&t.extends).chain(t.implements.iter()).collect();
            let Some(base) = bases
                .iter()
                .find(|b| REPOSITORY_BASES.contains(&simple_name(&strip_generics(b))))
            else {
                continue;
            };
            out.push(Repository {
                fqcn: t.fqcn.clone(),
                simple: t.name.clone(),
                entity: type_argument(base, 0).unwrap_or_default(),
                id_type: type_argument(base, 1).unwrap_or_default(),
                base: simple_name(&strip_generics(base)).to_string(),
                methods: repo_methods(t, &u.facts, &u.text),
                file: u.facts.file.clone(),
                offset: t.name_offset,
                line: line_at(&u.text, t.name_offset),
            });
        }
    }
    out
}

fn repo_methods(t: &TypeFacts, facts: &JavaFacts, text: &str) -> Vec<RepoMethod> {
    t.methods
        .iter()
        .filter(|m| !m.is_constructor)
        .map(|m| RepoMethod {
            name: m.name.clone(),
            return_type: m.return_type.clone(),
            params: m
                .params
                .iter()
                .map(|p| MethodParam {
                    name: p.name.clone(),
                    type_text: p.type_text.clone(),
                    bound_name: known::find(&p.annotations, facts, "Param")
                        .and_then(|a| a.value())
                        .map(|s| s.value.clone())
                        .unwrap_or_default(),
                    offset: p.name_offset,
                })
                .collect(),
            query: known::find(&m.annotations, facts, "Query").and_then(|a| query_def(a)),
            modifying: known::has(&m.annotations, facts, "Modifying"),
            offset: m.name_offset,
            line: line_at(text, m.name_offset),
        })
        .collect()
}

fn query_def(ann: &AnnFacts) -> Option<QueryDef> {
    // `value` (bare or named) is the query; `@Query(value = "…", nativeQuery = true)` is the
    // only shape in which a native one can be written, so the pair decides.
    let s = ann.value()?;
    let native = ann.pair("nativeQuery").is_some_and(|v| v.trim() == "true");
    let found = crate::hql::placeholders(&s.value);
    Some(QueryDef {
        named_params: found.iter().filter(|p| !p.positional).map(|p| p.name.clone()).collect(),
        positional_params: {
            let mut v: Vec<u32> =
                found.iter().filter(|p| p.positional).filter_map(|p| p.name.parse().ok()).collect();
            v.sort_unstable();
            v.dedup();
            v
        },
        text: s.value.clone(),
        start: s.start,
        end: s.end,
        native,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::scan_java;

    const IMPORTS: &str = "import jakarta.persistence.*; import org.springframework.data.jpa.repository.*; import org.springframework.data.repository.query.Param;";

    fn unit(src: &str) -> JavaUnit {
        let text = match src.find('\n') {
            Some(nl) if src.trim_start().starts_with("package") => {
                format!("{}{IMPORTS}{}", &src[..nl], &src[nl..])
            }
            _ => format!("{IMPORTS}\n{src}"),
        };
        JavaUnit { facts: scan_java("/p/T.java", &text).unwrap(), text }
    }

    #[test]
    fn an_entity_records_its_table_id_and_columns() {
        let u = unit(
            "package p;\n@Entity @Table(name = \"ORDINI\")\nclass Order {\n  @Id private Long id;\n  @Column(name = \"IMPORTO\") private java.math.BigDecimal total;\n  @Transient private String scratch;\n}\n",
        );
        let e = &entities(std::slice::from_ref(&u))[0];
        assert_eq!(e.simple, "Order");
        assert_eq!(e.table, "ORDINI");
        assert_eq!(e.kind, "entity");
        assert_eq!(e.id_field().unwrap().name, "id");
        assert_eq!(e.field("total").unwrap().column, "IMPORTO");
        assert!(e.field("scratch").unwrap().transient);
    }

    #[test]
    fn the_jpql_name_can_differ_from_the_class_name() {
        let u = unit("package p;\n@Entity(name = \"Ordine\") class Order { @Id Long id; }\n");
        assert_eq!(entities(std::slice::from_ref(&u))[0].entity_name, "Ordine");
    }

    /// A to-many relation's target is inside the collection, not the field type — getting this
    /// backwards makes every derived path through a collection unresolvable.
    #[test]
    fn a_relation_targets_the_element_type_for_a_collection() {
        let u = unit(
            "package p;\n@Entity class Order {\n  @ManyToOne private Customer customer;\n  @OneToMany private java.util.List<Line> lines;\n}\n",
        );
        let e = &entities(std::slice::from_ref(&u))[0];
        assert_eq!(e.field("customer").unwrap().target, "Customer");
        assert_eq!(e.field("lines").unwrap().target, "Line");
        assert_eq!(e.field("lines").unwrap().relation, "OneToMany");
        assert!(e.field("lines").unwrap().is_navigable());
    }

    #[test]
    fn embeddables_and_mapped_superclasses_are_entities_too() {
        let u = unit("package p;\n@MappedSuperclass class Base { @Id Long id; }\n");
        assert_eq!(entities(std::slice::from_ref(&u))[0].kind, "mapped-superclass");
    }

    /// The recognition rule with no annotation in sight.
    #[test]
    fn a_repository_is_recognised_by_what_it_extends() {
        let u = unit(
            "package p;\npublic interface OrderRepository extends JpaRepository<Order, Long> {}\n",
        );
        let r = &repositories(std::slice::from_ref(&u))[0];
        assert_eq!(r.entity, "Order");
        assert_eq!(r.id_type, "Long");
        assert_eq!(r.base, "JpaRepository");
    }

    #[test]
    fn a_query_method_records_its_text_params_and_flavour() {
        let u = unit(
            "package p;\ninterface R extends CrudRepository<Order, Long> {\n  @Query(\"select o from Order o where o.total > :min\") java.util.List<Order> rich(@Param(\"min\") int m);\n  @Query(value = \"select * from ORDINI where ID = ?1\", nativeQuery = true) Order raw(Long id);\n  @Modifying @Query(\"delete from Order o\") void wipe();\n}\n",
        );
        let r = &repositories(std::slice::from_ref(&u))[0];
        let by = |n: &str| r.methods.iter().find(|m| m.name == n).unwrap();

        let rich = by("rich").query.as_ref().unwrap();
        assert!(!rich.native);
        assert_eq!(rich.named_params, ["min"]);
        assert_eq!(by("rich").params[0].effective_name(), "min", "@Param renames it");

        let raw = by("raw").query.as_ref().unwrap();
        assert!(raw.native, "nativeQuery = true is the only thing that says so");
        assert_eq!(raw.positional_params, [1]);

        assert!(by("wipe").modifying);
    }

    /// The span must cover the query CONTENTS, not the Java string literal around it, or every
    /// highlight lands one character off.
    #[test]
    fn the_query_span_points_at_the_query_itself() {
        let u = unit("package p;\ninterface R extends JpaRepository<Order, Long> {\n  @Query(\"select o from Order o\") Object m();\n}\n");
        let r = &repositories(std::slice::from_ref(&u))[0];
        let q = r.methods[0].query.as_ref().unwrap();
        assert_eq!(&u.text[q.start..q.end], "select o from Order o");
    }

    #[test]
    fn a_plain_interface_is_not_a_repository() {
        let u = unit("package p;\npublic interface Helper { void go(); }\n");
        assert!(repositories(std::slice::from_ref(&u)).is_empty());
    }
}
