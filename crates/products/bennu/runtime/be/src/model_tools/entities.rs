//! `bennu_entities` — the persistence model: entities, their relations and the repositories over them.

use bennu_jpa::prelude::{parse_derived, simple_name, strip_generics, Entity, JpaModel, RepoMethod, Repository};
use serde::Serialize;

use super::has;

/// Methods listed per repository; the rest are counted.
const MAX_METHODS: usize = 40;
/// `@Query` texts are cut here — a report query can run to a page.
const MAX_QUERY_CHARS: usize = 300;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RelationRow {
    pub field: String,
    /// `OneToMany` | `ManyToOne` | `OneToOne` | `ManyToMany`.
    pub kind: String,
    pub target: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub join_column: String,
    /// Whether this side holds the foreign key.
    pub owning: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FieldRow {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub column: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub id: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub transient: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RepoMethodRow {
    pub name: String,
    pub returns: String,
    /// `derived` (the name is the query) | `jpql` | `native` | `other` (a plain or default method).
    pub kind: String,
    /// The `@Query` text, or a plain-words reading of a derived name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub query: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub modifying: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct RepoRow {
    pub name: String,
    pub base: String,
    pub id_type: String,
    pub methods: Vec<RepoMethodRow>,
    /// How many methods it declares; more than `methods` lists when the cap cut some.
    pub methods_total: usize,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct EntityRow {
    pub name: String,
    pub class: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub table: String,
    /// `entity` | `embeddable` | `mapped-superclass`.
    pub kind: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub extends: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub relations: Vec<RelationRow>,
    pub field_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<FieldRow>>,
    pub repositories: Vec<RepoRow>,
    pub file: String,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct EntityFilter {
    pub name: Option<String>,
    pub fields: bool,
}

fn method_row(m: &RepoMethod) -> RepoMethodRow {
    let (kind, query) = match (&m.query, parse_derived(&m.name)) {
        (Some(q), _) => (if q.native { "native" } else { "jpql" }, cut(&q.text)),
        (None, Some(d)) => ("derived", d.describe()),
        (None, None) => ("other", String::new()),
    };
    RepoMethodRow { name: m.name.clone(), returns: m.return_type.clone(), kind: kind.into(), query, modifying: m.modifying }
}

fn cut(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_QUERY_CHARS {
        return flat;
    }
    format!("{}…", flat.chars().take(MAX_QUERY_CHARS).collect::<String>())
}

fn repo_row(r: &Repository) -> RepoRow {
    RepoRow {
        name: r.fqcn.clone(),
        base: r.base.clone(),
        id_type: r.id_type.clone(),
        methods: r.methods.iter().take(MAX_METHODS).map(method_row).collect(),
        methods_total: r.methods.len(),
    }
}

fn entity_row(model: &JpaModel, e: &Entity, with_fields: bool) -> EntityRow {
    // Inherited fields count: an id on a `@MappedSuperclass` is the id of every entity below it.
    let fields = model.fields_of(e);
    let relations = fields
        .iter()
        .filter(|f| !f.relation.is_empty() && !f.transient)
        .map(|f| RelationRow {
            field: f.name.clone(),
            kind: f.relation.clone(),
            target: simple_name(&strip_generics(&f.target)).to_string(),
            join_column: f.join_column.clone(),
            owning: f.owns_column,
        })
        .collect();
    EntityRow {
        name: e.entity_name.clone(),
        class: e.fqcn.clone(),
        table: e.table.clone(),
        kind: e.kind.clone(),
        extends: e.extends.clone(),
        id: fields.iter().find(|f| f.is_id).map(|f| f.name.clone()),
        relations,
        field_count: fields.len(),
        fields: with_fields.then(|| {
            fields
                .iter()
                .map(|f| FieldRow { name: f.name.clone(), type_name: f.type_text.clone(), column: f.column.clone(), id: f.is_id, transient: f.transient })
                .collect()
        }),
        repositories: model.repositories_of(&e.simple).into_iter().map(repo_row).collect(),
        file: e.file.clone(),
        line: e.line,
    }
}

pub fn rows(model: &JpaModel, filter: &EntityFilter) -> Vec<EntityRow> {
    let mut rows: Vec<EntityRow> = model
        .entities
        .iter()
        .filter(|e| has(&e.entity_name, &filter.name) || has(&e.fqcn, &filter.name) || has(&e.table, &filter.name))
        .map(|e| entity_row(model, e, filter.fields))
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

pub fn collect(root: &str, filter: &EntityFilter) -> Vec<EntityRow> {
    crate::frameworks::jpa_model_of(root).map(|m| rows(&m, filter)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_jpa::prelude::{EntityField, QueryDef};

    fn field(name: &str, ty: &str) -> EntityField {
        EntityField { name: name.into(), type_text: ty.into(), ..EntityField::default() }
    }

    fn entity(name: &str, extends: &str, kind: &str, fields: Vec<EntityField>) -> Entity {
        Entity {
            fqcn: format!("com.acme.{name}"),
            simple: name.into(),
            entity_name: name.into(),
            table: name.to_lowercase(),
            kind: kind.into(),
            extends: extends.into(),
            fields,
            file: "/p/E.java".into(),
            ..Entity::default()
        }
    }

    fn model() -> JpaModel {
        let base = entity("Base", "", "mapped-superclass", vec![EntityField { is_id: true, ..field("id", "Long") }]);
        let order = entity(
            "Order",
            "Base",
            "entity",
            vec![
                field("total", "BigDecimal"),
                EntityField { relation: "ManyToOne".into(), target: "Customer".into(), owns_column: true, join_column: "customer_id".into(), ..field("customer", "Customer") },
                EntityField { relation: "OneToMany".into(), target: "List<Line>".into(), ..field("lines", "List<Line>") },
                EntityField { relation: "OneToOne".into(), target: "Audit".into(), transient: true, ..field("audit", "Audit") },
            ],
        );
        let repo = Repository {
            fqcn: "com.acme.OrderRepository".into(),
            simple: "OrderRepository".into(),
            entity: "Order".into(),
            id_type: "Long".into(),
            base: "JpaRepository".into(),
            methods: vec![
                RepoMethod { name: "findByCustomerName".into(), return_type: "List<Order>".into(), ..RepoMethod::default() },
                RepoMethod {
                    name: "report".into(),
                    return_type: "List<Object[]>".into(),
                    query: Some(QueryDef { text: "select   o\n from Order o".into(), native: false, ..QueryDef::default() }),
                    ..RepoMethod::default()
                },
                RepoMethod { name: "touch".into(), return_type: "void".into(), ..RepoMethod::default() },
            ],
            file: "/p/R.java".into(),
            offset: 0,
            line: 1,
        };
        JpaModel { entities: vec![base, order], repositories: vec![repo] }
    }

    #[test]
    fn an_entity_inherits_its_id_and_lists_only_real_relations() {
        let rows = rows(&model(), &EntityFilter { name: Some("order".into()), fields: false });
        assert_eq!(rows.len(), 1);
        let o = &rows[0];
        assert_eq!(o.id.as_deref(), Some("id"), "the id lives on the mapped superclass");
        assert_eq!(o.field_count, 5);
        let rel: Vec<_> = o.relations.iter().map(|r| (r.field.as_str(), r.kind.as_str(), r.target.as_str(), r.owning)).collect();
        assert_eq!(rel, vec![("customer", "ManyToOne", "Customer", true), ("lines", "OneToMany", "Line", false)], "transient relations are not mapped");
        assert!(o.fields.is_none());
    }

    #[test]
    fn repository_methods_are_classified_by_how_they_query() {
        let rows = rows(&model(), &EntityFilter { name: Some("order".into()), fields: true });
        let repo = &rows[0].repositories[0];
        let kinds: Vec<_> = repo.methods.iter().map(|m| m.kind.as_str()).collect();
        assert_eq!(kinds, vec!["derived", "jpql", "other"]);
        assert_eq!(repo.methods[1].query, "select o from Order o", "whitespace in a @Query is collapsed");
        assert!(!repo.methods[0].query.is_empty(), "a derived name is read back in words");
        assert_eq!(rows[0].fields.as_ref().unwrap().len(), 5);
    }

    #[test]
    fn a_long_query_is_cut_with_an_ellipsis() {
        let long = "x ".repeat(400);
        let cut = cut(&long);
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), MAX_QUERY_CHARS + 1);
    }
}
