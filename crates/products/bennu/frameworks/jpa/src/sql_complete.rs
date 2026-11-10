//! Completion inside a **native** SQL string — tables after `from`, columns after `alias.`, and the
//! keywords everywhere else.
//!
//! The vocabulary is the project's own mapping: a table is an entity's `@Table` (or the name JPA
//! would derive), a column is a field's `@Column` / `@JoinColumn` (or the derived one). That is what
//! this crate can state *without a database*; a table the code never maps is simply not offered,
//! and nothing is ever claimed about one that is typed by hand. The live schema is Picus's business.
//!
//! Same discipline as [`crate::hql_complete`]: offer only what is certain. A relation that does not
//! own a column (`mappedBy`, a collection) and an `@Embedded` object (whose columns are the
//! embeddable's) contribute none, since guessing them would put a column in the popup that is not
//! in the table.

use bennu_proto::prelude::CompletionItem;

use crate::hql_complete::{item, tokens};
use crate::model::{Entity, EntityField, JpaModel};

/// Keywords worth offering in a native query — a closed set; the popup is for the project's names.
const KEYWORDS: &[&str] = &[
    "select", "from", "where", "and", "or", "not", "in", "is", "null", "like", "between", "exists",
    "join", "left", "right", "inner", "outer", "cross", "full", "on", "group", "by", "order",
    "having", "limit", "offset", "union", "all", "distinct", "as", "case", "when", "then", "else",
    "end", "asc", "desc", "insert", "into", "values", "update", "set", "delete", "with", "returning",
    "using", "fetch", "first", "rows", "only", "over", "partition",
];

/// Words that can follow a table and are therefore not its alias.
const CLAUSE_WORDS: &[&str] = &[
    "where", "on", "join", "left", "right", "inner", "outer", "cross", "full", "natural", "group",
    "order", "having", "limit", "offset", "union", "set", "using", "values", "returning", "fetch",
    "for", "window", "select", "intersect", "except", "minus",
];

/// A table the code maps, with the entity behind it.
struct Table<'a> {
    /// `@Table(name = …)` as written, or the derived `UPPER_SNAKE` form when defaulted.
    written: String,
    explicit: bool,
    entity: &'a Entity,
}

impl Table<'_> {
    fn is_named(&self, name: &str) -> bool {
        self.written.eq_ignore_ascii_case(name)
    }
}

fn tables(model: &JpaModel) -> Vec<Table<'_>> {
    model
        .entities
        .iter()
        .filter(|e| e.kind == "entity")
        .map(|e| {
            let explicit = !e.table.is_empty();
            let written = if explicit { e.table.clone() } else { snake_upper(&e.simple) };
            Table { written, explicit, entity: e }
        })
        .collect()
}

/// `createdAt` → `CREATED_AT`: what a mapping with no `@Table` / `@Column` ends up named.
fn snake_upper(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.char_indices() {
        if c.is_uppercase() && i > 0 && !out.ends_with('_') {
            out.push('_');
        }
        out.extend(c.to_uppercase());
    }
    out
}

/// A name as it should be typed: explicit ones verbatim, derived ones in the case being typed.
fn spelled(written: &str, explicit: bool, shout: bool) -> String {
    if explicit || shout { written.to_string() } else { written.to_lowercase() }
}

/// The column a field owns, with the text to show beside it. `None` when the field has no column
/// of its own that can be named with certainty.
fn column_of(field: &EntityField) -> Option<(String, bool, String)> {
    if field.transient || field.embedded {
        return None;
    }
    if !field.relation.is_empty() {
        if !field.owns_column {
            return None;
        }
        let (written, explicit) = if field.join_column.is_empty() {
            (format!("{}_ID", snake_upper(&field.name)), false)
        } else {
            (field.join_column.clone(), true)
        };
        return Some((written, explicit, format!("→ {}", field.target)));
    }
    let (written, explicit) = if field.column.is_empty() {
        (snake_upper(&field.name), false)
    } else {
        (field.column.clone(), true)
    };
    Some((written, explicit, field.type_text.clone()))
}

fn columns(model: &JpaModel, table: &Table<'_>, shout: bool) -> Vec<CompletionItem> {
    model
        .fields_of(table.entity)
        .into_iter()
        .filter_map(column_of)
        .map(|(written, explicit, detail)| item(&spelled(&written, explicit, shout), "field", Some(detail)))
        .collect()
}

fn table_items(tables: &[Table<'_>], shout: bool) -> Vec<CompletionItem> {
    tables
        .iter()
        .map(|t| item(&spelled(&t.written, t.explicit, shout), "class", Some(t.entity.fqcn.clone())))
        .collect()
}

/// Every `from orders o` / `join customer as c` / `update orders` of the query, as
/// (alias-or-name, index into `tables`). Read over the whole text: the `from` clause normally
/// comes after the `select` list the caret is in.
fn scope(tables: &[Table<'_>], text: &str) -> Vec<(String, usize)> {
    let toks = tokens(text);
    let mut out = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if !matches!(toks[i].to_lowercase().as_str(), "from" | "join" | "update" | "into") {
            i += 1;
            continue;
        }
        i += 1;
        loop {
            let Some(source) = toks.get(i) else { break };
            i += 1;
            let bare = source.rsplit('.').next().unwrap_or(source);
            let found = tables.iter().position(|t| t.is_named(bare));
            if toks.get(i).is_some_and(|t| t.eq_ignore_ascii_case("as")) {
                i += 1;
            }
            let alias = toks.get(i).filter(|a| !CLAUSE_WORDS.contains(&a.to_lowercase().as_str()) && *a != ",");
            if let Some(at) = found {
                out.push((bare.to_lowercase(), at));
                if let Some(alias) = alias {
                    out.push((alias.to_lowercase(), at));
                }
            }
            if alias.is_some() {
                i += 1;
            }
            // `from a x, b y` — another source follows the comma.
            if toks.get(i).is_some_and(|t| t == ",") {
                i += 1;
                continue;
            }
            break;
        }
    }
    out
}

/// What can be typed at the caret: the prefix being typed (so the caller can filter and replace
/// it) and the unfiltered candidates. `lead` is the text before the word, `word` the (possibly
/// dotted) word under the caret.
pub(crate) fn candidates<'w>(
    model: &JpaModel,
    text: &str,
    lead: &str,
    word: &'w str,
) -> (&'w str, Vec<CompletionItem>) {
    let tables = tables(model);
    let prefix = word.rsplit_once('.').map_or(word, |(_, p)| p);
    // Typing in capitals means the script is written in capitals: derived names follow.
    let shout = prefix.chars().any(char::is_alphabetic) && !prefix.chars().any(char::is_lowercase);
    let previous = lead.split_whitespace().last().unwrap_or("").to_lowercase();
    let names_a_table = matches!(previous.as_str(), "from" | "join" | "update" | "into" | "table");

    if let Some((qualifier, _)) = word.rsplit_once('.') {
        let last = qualifier.rsplit('.').next().unwrap_or(qualifier).to_lowercase();
        let in_scope = scope(&tables, text);
        let at = in_scope.iter().find(|(a, _)| *a == last).map(|(_, at)| *at);
        return match at {
            Some(at) => (prefix, columns(model, &tables[at], shout)),
            // `public.ord` after `from`: a schema qualifier, then a table.
            None if names_a_table => (prefix, table_items(&tables, shout)),
            None => (prefix, Vec::new()),
        };
    }
    if names_a_table {
        return (word, table_items(&tables, shout));
    }

    let in_scope = scope(&tables, text);
    let mut out: Vec<CompletionItem> = Vec::new();
    let mut seen_tables: Vec<usize> = Vec::new();
    for (alias, at) in &in_scope {
        out.push(item(alias, "variable", Some(tables[*at].written.clone())));
        if !seen_tables.contains(at) {
            seen_tables.push(*at);
        }
    }
    // The columns of every table in the query, so `where tot|` finds `total` without an alias.
    for at in seen_tables {
        for column in columns(model, &tables[at], shout) {
            if !out.iter().any(|o| o.label == column.label && o.kind == "field") {
                out.push(column);
            }
        }
    }
    out.extend(KEYWORDS.iter().map(|k| {
        let label = if shout { k.to_uppercase() } else { (*k).to_string() };
        item(&label, "keyword", None)
    }));
    (word, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hql_complete::completions;
    use crate::model::QueryDef;

    fn field(name: &str, ty: &str) -> EntityField {
        EntityField { name: name.into(), type_text: ty.into(), ..EntityField::default() }
    }

    fn model() -> JpaModel {
        let customer = EntityField {
            relation: "ManyToOne".into(),
            target: "Customer".into(),
            owns_column: true,
            ..field("customer", "Customer")
        };
        let items = EntityField { relation: "OneToMany".into(), target: "Item".into(), ..field("items", "List<Item>") };
        JpaModel {
            entities: vec![
                Entity {
                    fqcn: "com.acme.Order".into(),
                    simple: "Order".into(),
                    entity_name: "Order".into(),
                    table: "ORDINI".into(),
                    kind: "entity".into(),
                    fields: vec![
                        field("total", "BigDecimal"),
                        EntityField { column: "DATA_ORD".into(), ..field("placedAt", "Instant") },
                        customer,
                        items,
                        EntityField { transient: true, ..field("scratch", "String") },
                    ],
                    ..Entity::default()
                },
                Entity {
                    fqcn: "com.acme.Customer".into(),
                    simple: "Customer".into(),
                    entity_name: "Customer".into(),
                    kind: "entity".into(),
                    fields: vec![field("fullName", "String")],
                    ..Entity::default()
                },
            ],
            ..JpaModel::default()
        }
    }

    /// `text` with `|` marking the caret, completed as a native query at offset 10 of a pretend file.
    fn at(text: &str) -> Vec<String> {
        let caret = text.find('|').unwrap();
        let text = text.replacen('|', "", 1);
        let source = format!("0123456789{text}\")");
        let q = QueryDef { start: 10, end: 10 + text.len(), text: text.clone(), native: true, ..QueryDef::default() };
        completions(&model(), &[], &q, &source, 10 + caret).into_iter().map(|c| c.label).collect()
    }

    #[test]
    fn a_mapped_table_is_offered_after_from() {
        assert_eq!(at("select * from ORD|"), ["ORDINI"]);
        // An unmapped table is not invented; a derived name follows the case being typed.
        assert_eq!(at("select * from cust|"), ["customer"]);
        assert_eq!(at("select * from CUST|"), ["CUSTOMER"]);
    }

    #[test]
    fn columns_follow_an_alias_and_come_from_the_mapping() {
        assert_eq!(at("select o.| from ORDINI o"), ["total", "DATA_ORD", "customer_id"]);
        // The collection owns no column here and the transient field none anywhere.
        assert!(!at("select o.| from ORDINI o").iter().any(|l| l.eq_ignore_ascii_case("items") || l == "SCRATCH"));
    }

    #[test]
    fn the_table_name_works_as_a_qualifier_and_a_join_brings_its_alias() {
        assert_eq!(at("select ORDINI.DA| from ORDINI"), ["DATA_ORD"]);
        assert_eq!(at("select c.full| from ORDINI o join customer c on c.id = o.customer_id"), ["full_name"]);
    }

    #[test]
    fn bare_columns_of_the_tables_in_the_query_are_offered_with_keywords() {
        let items = at("select * from ORDINI where DATA|");
        assert_eq!(items, ["DATA_ORD"]);
        assert!(at("select * from ORDINI wh|").contains(&"where".to_string()));
        assert!(at("SELECT * FROM ORDINI WH|").contains(&"WHERE".to_string()));
    }

    #[test]
    fn nothing_is_offered_inside_a_string_literal() {
        assert!(at("select * from ORDINI where DATA_ORD = 'a o.|").is_empty());
    }

    #[test]
    fn a_schema_qualifier_still_reaches_the_tables() {
        assert_eq!(at("select * from public.ORD|"), ["ORDINI"]);
    }
}
