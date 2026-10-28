//! Field names in a JPQL / HQL query, checked against the entity model.
//!
//! `select o from Order o where o.totl > 1` is not a Java error and not a SQL one: the provider
//! parses the string when the context starts (Spring Data) or when the query first runs, and the
//! typo surfaces there. Every `alias.field` whose alias this query declares, and whose entity the
//! model knows, is looked up — and a field that is not there is reported where it is written.
//!
//! The same discipline as the rest of the crate: **report only what is certain.** A path is judged
//! only while every step lands on a type the model has scanned, and an entity whose inheritance chain
//! leaves the project is not judged at all (its fields are unknown). Nothing here knows the schema —
//! whether a *column* exists is the database's answer, given by the native-query check.

use crate::derived::chain_is_complete;
use crate::hql_complete::aliases;
use crate::model::{strip_generics, JpaModel};

/// A field that is not there, as a span of the query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownField {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// Every `alias.field` of `text` that names a field the entity does not have.
pub fn unknown_fields(model: &JpaModel, text: &str) -> Vec<UnknownField> {
    let scope = aliases(model, text);
    if scope.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\'' {
            // A string literal: `''` is an escaped quote and keeps the parity.
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\'' {
                    if bytes.get(i + 1) == Some(&b'\'') {
                        i += 2;
                        continue;
                    }
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        let starts_word = c.is_ascii_alphabetic() || c == b'_';
        // A word begun mid-token (`1.5`, `x.y`'s tail) or after `:` / `?` (a parameter) is not one.
        let glued = i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || matches!(bytes[i - 1], b'_' | b'.' | b':' | b'?'));
        if !starts_word || glued {
            i += 1;
            continue;
        }
        // The dotted word: `o.customer.name`, with where each segment sits.
        let mut segments: Vec<(usize, usize)> = Vec::new();
        let mut j = i;
        loop {
            let s = j;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            segments.push((s, j));
            if bytes.get(j) == Some(&b'.')
                && bytes.get(j + 1).is_some_and(|n| n.is_ascii_alphabetic() || *n == b'_')
            {
                j += 1;
                continue;
            }
            break;
        }
        if segments.len() >= 2 {
            check_path(model, &scope, text, &segments, &mut out);
        }
        i = j.max(i + 1);
    }
    out
}

fn check_path<'a>(
    model: &'a JpaModel,
    scope: &crate::hql_complete::Aliases<'a>,
    text: &str,
    segments: &[(usize, usize)],
    out: &mut Vec<UnknownField>,
) {
    let Some(mut current) = scope.get(&text[segments[0].0..segments[0].1]).copied() else { return };
    for (n, &(s, e)) in segments.iter().enumerate().skip(1) {
        let name = &text[s..e];
        let fields = model.fields_of(current);
        let Some(field) = fields.iter().find(|f| f.name == name && !f.transient) else {
            // `class` (the discriminator) and `id` (the identifier, whatever the field is called) are
            // special in HQL; and an entity whose parents are out of the project has unknown fields.
            if name != "class" && name != "id" && chain_is_complete(model, current) {
                out.push(UnknownField {
                    start: s,
                    end: e,
                    message: format!(
                        "`{name}` is not a field of {} — the provider rejects this query when it parses it.",
                        current.entity_name,
                    ),
                });
            }
            return;
        };
        if n + 1 == segments.len() {
            return;
        }
        // Another step: through a relation to its target, or into an embedded object. A type the
        // model has not scanned ends the walk without a verdict.
        let target = if field.relation.is_empty() { strip_generics(&field.type_text) } else { field.target.clone() };
        let Some(next) = model.entity(&target) else { return };
        current = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, EntityField};

    fn field(name: &str, ty: &str, relation: &str, target: &str) -> EntityField {
        EntityField {
            name: name.into(),
            type_text: ty.into(),
            relation: relation.into(),
            target: target.into(),
            ..EntityField::default()
        }
    }

    fn model() -> JpaModel {
        let entity = |name: &str, kind: &str, fields: Vec<EntityField>| Entity {
            fqcn: format!("com.acme.{name}"),
            simple: name.into(),
            entity_name: name.into(),
            kind: kind.into(),
            fields,
            ..Entity::default()
        };
        JpaModel {
            entities: vec![
                entity(
                    "Order",
                    "entity",
                    vec![
                        field("total", "BigDecimal", "", ""),
                        field("customer", "Customer", "ManyToOne", "Customer"),
                        field("key", "OrderKey", "", ""),
                    ],
                ),
                entity("Customer", "entity", vec![field("name", "String", "", "")]),
                entity("OrderKey", "embeddable", vec![field("code", "String", "", "")]),
            ],
            ..JpaModel::default()
        }
    }

    fn unknown(text: &str) -> Vec<String> {
        unknown_fields(&model(), text).into_iter().map(|u| text[u.start..u.end].to_string()).collect()
    }

    #[test]
    fn a_misspelled_field_is_reported_where_it_is_written() {
        assert_eq!(unknown("select o from Order o where o.totl > 1"), vec!["totl"]);
    }

    #[test]
    fn a_relation_is_followed_and_an_embedded_object_entered() {
        assert!(unknown("select o from Order o where o.customer.name = :n and o.key.code = :c").is_empty());
        assert_eq!(unknown("select o from Order o where o.customer.nme = :n"), vec!["nme"]);
        assert_eq!(unknown("select o from Order o where o.key.cod = :c"), vec!["cod"]);
    }

    #[test]
    fn a_joined_alias_is_known() {
        assert_eq!(unknown("select o from Order o join o.customer c where c.nam = 'x'"), vec!["nam"]);
    }

    #[test]
    fn the_identifier_and_the_discriminator_are_always_there() {
        assert!(unknown("select o from Order o where o.id = 1 and o.class = Order").is_empty());
    }

    #[test]
    fn a_string_a_number_and_a_parameter_are_not_paths() {
        assert!(unknown("select o from Order o where o.total > 1.5 and :o.x = 'o.nope'").is_empty());
    }

    #[test]
    fn a_path_through_a_type_the_model_never_saw_is_not_judged() {
        let mut m = model();
        m.entities[0].fields.push(field("meta", "ForeignType", "", ""));
        assert!(unknown_fields(&m, "select o from Order o where o.meta.anything = 1").is_empty());
    }

    #[test]
    fn an_alias_this_query_does_not_declare_is_not_judged() {
        assert!(unknown("select x.y from Order o").is_empty());
    }
}
