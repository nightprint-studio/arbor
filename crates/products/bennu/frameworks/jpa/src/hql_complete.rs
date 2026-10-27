//! Completion inside a JPQL `@Query` — entity names after `from`, fields after `alias.`, the
//! method's parameters after `:`, and the keywords everywhere else.
//!
//! The same discipline as the rest of this crate: **offer only what is certain.** A name is
//! suggested when the model says it exists; nothing is suggested inside a string literal of the
//! query, in a native query (the identifiers there are tables and columns, which this crate does
//! not know), or when the buffer no longer holds the query the model saw — a completion that
//! replaces the wrong span corrupts the code.

use std::collections::HashMap;

use bennu_proto::prelude::CompletionItem;

use crate::hql;
use crate::model::{Entity, JpaModel, MethodParam, QueryDef};

/// A completion popup past this is not something anyone reads.
const MAX_ITEMS: usize = 200;

/// The candidates at `offset` (a byte offset in the file) inside `query`, whose Java string literal
/// starts at `query.start`.
pub fn completions(
    model: &JpaModel,
    params: &[MethodParam],
    query: &QueryDef,
    source: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    if query.native || offset < query.start || offset > query.end {
        return Vec::new();
    }
    // The query is read off the live buffer, but guard the span anyway.
    if source.get(query.start..query.end) != Some(query.text.as_str()) {
        return Vec::new();
    }
    let caret = offset - query.start;
    let Some(before) = query.text.get(..caret) else { return Vec::new() };
    if inside_literal(before) {
        return Vec::new();
    }

    let word_start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |i| i + before[i..].chars().next().map_or(1, char::len_utf8));
    let word = &before[word_start..];
    let lead = &before[..word_start];
    let scope = aliases(model, &query.text);

    let (prefix, items) = if lead.ends_with(':') {
        (word, parameters(params))
    } else if let Some((qualifier, prefix)) = word.rsplit_once('.') {
        (prefix, members(model, &scope, qualifier))
    } else {
        (word, names(model, &scope, lead))
    };

    let wanted = prefix.to_lowercase();
    let from = offset - prefix.len();
    let mut out: Vec<CompletionItem> = items
        .into_iter()
        .filter(|i| i.label.to_lowercase().starts_with(&wanted))
        .take(MAX_ITEMS)
        .collect();
    for item in &mut out {
        item.replace_start = Some(from);
        item.replace_end = Some(offset);
    }
    out
}

/// Whether the caret sits inside a `'…'` literal of the query (`''` is an escaped quote and keeps
/// the parity honest by counting twice).
fn inside_literal(before: &str) -> bool {
    before.chars().filter(|c| *c == '\'').count() % 2 == 1
}

fn item(label: &str, kind: &str, detail: Option<String>) -> CompletionItem {
    CompletionItem { label: label.to_string(), kind: kind.to_string(), detail, ..Default::default() }
}

fn parameters(params: &[MethodParam]) -> Vec<CompletionItem> {
    params
        .iter()
        .filter(|p| !is_paging(&p.type_text))
        .map(|p| item(p.effective_name(), "variable", Some(p.type_text.clone())))
        .collect()
}

/// `Pageable` and `Sort` are consumed by Spring Data, never bound to a placeholder.
fn is_paging(type_text: &str) -> bool {
    matches!(type_text.trim(), "Pageable" | "Sort")
}

/// The fields addressable through `qualifier` — an alias, optionally followed by a relation path.
fn members(model: &JpaModel, aliases: &Aliases, qualifier: &str) -> Vec<CompletionItem> {
    let Some(entity) = resolve_path(model, aliases, qualifier) else { return Vec::new() };
    model
        .fields_of(entity)
        .into_iter()
        .filter(|f| !f.transient)
        .map(|f| {
            let detail = if f.relation.is_empty() {
                f.type_text.clone()
            } else {
                format!("{} → {}", f.relation, f.target)
            };
            item(&f.name, "field", Some(detail))
        })
        .collect()
}

/// What can start a word at the caret, decided by the keyword before it.
fn names(model: &JpaModel, aliases: &Aliases, lead: &str) -> Vec<CompletionItem> {
    let previous = lead.split_whitespace().last().unwrap_or("").to_lowercase();
    let entities = || {
        model
            .entities
            .iter()
            .filter(|e| e.kind == "entity")
            .map(|e| item(&e.entity_name, "class", Some(e.fqcn.clone())))
    };
    let aliased = || {
        aliases.iter().map(|(a, e)| item(a, "variable", Some(e.entity_name.clone())))
    };
    match previous.as_str() {
        "from" | "update" => entities().collect(),
        "join" | "fetch" | "in" => aliased().collect(),
        _ => aliased().chain(hql::jpql_keywords().iter().map(|k| item(k, "keyword", None))).collect(),
    }
}

type Aliases<'a> = HashMap<String, &'a Entity>;

/// Follow `alias.relation.relation` to the entity at its end.
fn resolve_path<'a>(model: &'a JpaModel, aliases: &Aliases<'a>, path: &str) -> Option<&'a Entity> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut current = *aliases.get(first)?;
    for seg in segments {
        let field = model.fields_of(current).into_iter().find(|f| f.name == seg && !f.transient)?;
        if !field.is_navigable() {
            return None;
        }
        current = model.entity(&field.target)?;
    }
    Some(current)
}

/// Every `from Order o`, `join o.items i` and `update Order o` of the query, as alias → entity.
///
/// Read over the whole text rather than the part before the caret: the `from` clause normally
/// comes after the `select` list the caret is in.
fn aliases<'a>(model: &'a JpaModel, text: &str) -> Aliases<'a> {
    let tokens = tokens(text);
    let mut out: Aliases<'a> = HashMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i].to_lowercase();
        if matches!(t.as_str(), "from" | "update" | "join") {
            i += 1;
            loop {
                if tokens.get(i).is_some_and(|t| t.eq_ignore_ascii_case("fetch")) {
                    i += 1;
                }
                let Some(source) = tokens.get(i) else { break };
                i += 1;
                let entity = if source.contains('.') {
                    resolve_path(model, &out, source)
                } else {
                    model.entity(source)
                };
                if tokens.get(i).is_some_and(|t| t.eq_ignore_ascii_case("as")) {
                    i += 1;
                }
                if let (Some(entity), Some(alias)) = (entity, tokens.get(i)) {
                    if !is_keyword(alias) {
                        out.insert(alias.clone(), entity);
                        i += 1;
                    }
                }
                // `from A a, B b` — another source follows the comma.
                if tokens.get(i).is_some_and(|t| t.as_str() == ",") && t != "join" {
                    i += 1;
                    continue;
                }
                break;
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_keyword(word: &str) -> bool {
    let lower = word.to_lowercase();
    hql::jpql_keywords().contains(&lower.as_str())
}

/// Words (dots kept inside, so `o.items` is one) and commas, skipping string literals.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_literal = false;
    for c in text.chars() {
        if c == '\'' {
            in_literal = !in_literal;
            flush(&mut word, &mut out);
        } else if in_literal {
            continue;
        } else if c.is_alphanumeric() || c == '_' || c == '.' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            if c == ',' {
                out.push(",".to_string());
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

fn flush(word: &mut String, out: &mut Vec<String>) {
    if !word.is_empty() {
        out.push(std::mem::take(word));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EntityField;

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
        JpaModel {
            entities: vec![
                Entity {
                    fqcn: "com.acme.Order".into(),
                    simple: "Order".into(),
                    entity_name: "Order".into(),
                    kind: "entity".into(),
                    fields: vec![
                        field("total", "BigDecimal", "", ""),
                        field("customer", "Customer", "ManyToOne", "Customer"),
                        field("scratch", "String", "", ""),
                    ],
                    ..Entity::default()
                },
                Entity {
                    fqcn: "com.acme.Customer".into(),
                    simple: "Customer".into(),
                    entity_name: "Customer".into(),
                    kind: "entity".into(),
                    fields: vec![field("name", "String", "", "")],
                    ..Entity::default()
                },
            ],
            ..JpaModel::default()
        }
    }

    /// `text` with `|` marking the caret, as a query at offset 10 of a pretend file.
    fn at(text: &str, params: &[MethodParam]) -> Vec<String> {
        let caret = text.find('|').unwrap();
        let text = text.replacen('|', "", 1);
        let source = format!("0123456789{text}\")");
        let q = QueryDef { start: 10, end: 10 + text.len(), text: text.clone(), ..QueryDef::default() };
        completions(&model(), params, &q, &source, 10 + caret)
            .into_iter()
            .map(|c| c.label)
            .collect()
    }

    #[test]
    fn an_entity_is_offered_after_from() {
        assert_eq!(at("select o from Or|", &[]), ["Order"]);
        assert_eq!(at("select o from |", &[]).len(), 2);
    }

    #[test]
    fn fields_follow_the_alias_even_when_from_comes_after_the_caret() {
        assert_eq!(at("select o.to| from Order o", &[]), ["total"]);
        assert_eq!(at("select o from Order o where o.|", &[]), ["total", "customer", "scratch"]);
    }

    #[test]
    fn a_relation_path_continues_into_the_target() {
        assert_eq!(at("select o from Order o where o.customer.na|", &[]), ["name"]);
    }

    #[test]
    fn a_join_introduces_its_own_alias() {
        assert_eq!(at("select c from Order o join o.customer c where c.|", &[]), ["name"]);
    }

    #[test]
    fn parameters_are_offered_after_a_colon_and_paging_is_not_one() {
        let p = |name: &str, ty: &str| MethodParam { name: name.into(), type_text: ty.into(), ..MethodParam::default() };
        let params = [p("min", "BigDecimal"), p("pageable", "Pageable")];
        assert_eq!(at("select o from Order o where o.total > :|", &params), ["min"]);
    }

    #[test]
    fn nothing_is_offered_inside_a_literal_or_a_native_query() {
        assert!(at("select o from Order o where o.name = 'a o.|", &[]).is_empty());
        let q = QueryDef { start: 0, end: 5, text: "x".into(), native: true, ..QueryDef::default() };
        assert!(completions(&model(), &[], &q, "x", 1).is_empty());
    }

    #[test]
    fn a_buffer_that_no_longer_holds_the_query_is_left_alone() {
        let q = QueryDef { start: 0, end: 7, text: "from Or".into(), ..QueryDef::default() };
        assert!(completions(&model(), &[], &q, "edited!", 7).is_empty());
    }

    #[test]
    fn the_replaced_span_is_the_typed_prefix_only() {
        let text = "select o from Order o where o.to";
        let source = format!("0123456789{text}\")");
        let q = QueryDef { start: 10, end: 10 + text.len(), text: text.into(), ..QueryDef::default() };
        let items = completions(&model(), &[], &q, &source, 10 + text.len());
        assert_eq!(items[0].replace_start, Some(10 + text.len() - 2));
    }
}
