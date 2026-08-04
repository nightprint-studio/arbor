//! Derived query methods — the ones whose **name is the query**.
//!
//! `findByCustomerNameAndTotalGreaterThan` is not a name, it is a parsed expression: a subject,
//! a predicate over property paths, keywords, and an ordering. Spring Data compiles it at
//! **application start**, which is why this module exists — a typo in one is invisible to the
//! compiler, invisible to every test that does not touch that repository, and then it takes the
//! whole context down on deploy with `No property 'custmer' found for type Order`.
//!
//! Catching that at the caret is the single most valuable thing this crate does.
//!
//! ## What is checked, and what is deliberately not
//!
//! Every segment of the name must be a real property path on the managed entity, following
//! relations as it goes (`CustomerName` → `customer.name`). Spring's own resolution is greedy
//! and so is this one: the whole segment is tried as a single property first, then split from
//! the right — which is what makes `customerName` on `Order` win over `customer.name` when both
//! could exist, exactly as at runtime.
//!
//! The check goes **quiet** rather than guess when it cannot see the whole picture: an entity
//! whose `@MappedSuperclass` chain leaves the project, a relation whose target is not a scanned
//! type. Under-reporting is the house rule ([`crate::lib`]), and here it is also the difference
//! between a tool people leave on and one they turn off.

use crate::model::{capitalize, decapitalize, Entity, JpaModel};

/// What the method does — the part before `By`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    Find,
    Count,
    Exists,
    Delete,
}

impl Subject {
    pub fn as_str(&self) -> &'static str {
        match self {
            Subject::Find => "find",
            Subject::Count => "count",
            Subject::Exists => "exists",
            Subject::Delete => "delete",
        }
    }
}

/// The subject prefixes Spring Data accepts, longest first so `readBy` is not read as `read`
/// plus a stray `By`.
const SUBJECTS: &[(&str, Subject)] = &[
    ("findDistinct", Subject::Find),
    ("readDistinct", Subject::Find),
    ("queryDistinct", Subject::Find),
    ("getDistinct", Subject::Find),
    ("streamDistinct", Subject::Find),
    ("searchDistinct", Subject::Find),
    ("countDistinct", Subject::Count),
    ("find", Subject::Find),
    ("read", Subject::Find),
    ("query", Subject::Find),
    ("get", Subject::Find),
    ("stream", Subject::Find),
    ("search", Subject::Find),
    ("count", Subject::Count),
    ("exists", Subject::Exists),
    ("delete", Subject::Delete),
    ("remove", Subject::Delete),
];

/// Predicate keywords, **longest first**: `GreaterThanEqual` must be stripped before
/// `GreaterThan`, or every `>=` in the project reads as a `>` over a property called `Equal`.
///
/// The second field is how many method arguments the keyword consumes — the number that makes
/// the arity check possible, and the reason `Between` (two) and `IsNull` (none) are not
/// footnotes but data.
const KEYWORDS: &[(&str, usize)] = &[
    ("IsGreaterThanEqual", 1),
    ("IsLessThanEqual", 1),
    ("GreaterThanEqual", 1),
    ("LessThanEqual", 1),
    ("IsGreaterThan", 1),
    ("IsLessThan", 1),
    ("IsStartingWith", 1),
    ("IsEndingWith", 1),
    ("IsNotContaining", 1),
    ("IsContaining", 1),
    ("NotContaining", 1),
    ("StartingWith", 1),
    ("EndingWith", 1),
    ("GreaterThan", 1),
    ("IsNotEmpty", 0),
    ("IsNotNull", 0),
    ("LessThan", 1),
    ("Containing", 1),
    ("IsBetween", 2),
    ("NotBetween", 2),
    ("IsNotLike", 1),
    ("IsEmpty", 0),
    ("NotEmpty", 0),
    ("IsNotIn", 1),
    ("Contains", 1),
    ("Between", 2),
    ("IsNull", 0),
    ("NotNull", 0),
    ("NotLike", 1),
    ("IsTrue", 0),
    ("IsFalse", 0),
    ("Before", 1),
    ("After", 1),
    ("NotIn", 1),
    ("Equals", 1),
    ("IsLike", 1),
    ("Regex", 1),
    ("Like", 1),
    ("Null", 0),
    ("True", 0),
    ("False", 0),
    ("Not", 1),
    ("In", 1),
    ("Is", 1),
];

/// One condition of the predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Predicate {
    /// The segment as written (`CustomerNameGreaterThan`).
    pub raw: String,
    /// The property path it addresses, resolved (`["customer", "name"]`). Empty when it could
    /// not be resolved — see [`Issue`].
    pub path: Vec<String>,
    /// The comparison keyword (`GreaterThan`), empty for a plain equality.
    pub keyword: String,
    /// How many method arguments this condition consumes.
    pub args: usize,
    /// Whether it carries `IgnoreCase`.
    pub ignore_case: bool,
    /// Joined to the previous condition with `Or` rather than `And`.
    pub or: bool,
}

/// One `OrderBy` term.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderTerm {
    pub raw: String,
    pub path: Vec<String>,
    /// `true` for `Desc`.
    pub descending: bool,
}

/// A parsed derived query method name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedQuery {
    pub subject: Subject,
    pub distinct: bool,
    /// `Top10` / `First5` — the limit, when written.
    pub limit: Option<u32>,
    pub predicates: Vec<Predicate>,
    pub order_by: Vec<OrderTerm>,
}

impl DerivedQuery {
    /// How many bound arguments the name asks for. The number a method's parameter list must
    /// match — `Between` wants two, `IsNull` wants none.
    pub fn expected_args(&self) -> usize {
        self.predicates.iter().map(|p| p.args).sum()
    }

    /// A one-line rendering for a hover card: what this method actually asks the database.
    pub fn describe(&self) -> String {
        let subject = match (self.subject, self.distinct, self.limit) {
            (s, false, None) => s.as_str().to_string(),
            (s, true, None) => format!("{} distinct", s.as_str()),
            (s, false, Some(n)) => format!("{} first {n}", s.as_str()),
            (s, true, Some(n)) => format!("{} first {n} distinct", s.as_str()),
        };
        let mut out = subject;
        for (i, p) in self.predicates.iter().enumerate() {
            out.push(' ');
            if i > 0 {
                out.push_str(if p.or { "or " } else { "and " });
            } else {
                out.push_str("where ");
            }
            out.push_str(&p.path.join("."));
            if !p.keyword.is_empty() {
                out.push(' ');
                out.push_str(&spaced(&p.keyword));
            }
            if p.ignore_case {
                out.push_str(" (ignoring case)");
            }
        }
        for (i, o) in self.order_by.iter().enumerate() {
            out.push_str(if i == 0 { " ordered by " } else { ", " });
            out.push_str(&o.path.join("."));
            if o.descending {
                out.push_str(" desc");
            }
        }
        out
    }
}

/// `GreaterThanEqual` → `greater than equal`, for prose.
fn spaced(keyword: &str) -> String {
    let mut out = String::new();
    for (i, c) in keyword.char_indices() {
        if c.is_uppercase() && i > 0 {
            out.push(' ');
        }
        out.extend(c.to_lowercase());
    }
    out
}

/// A problem with a derived name, held to the never-false-positive standard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// The offending segment as written, for pointing at it.
    pub segment: String,
    pub message: String,
}

/// Parse a method name as a derived query, or `None` when it is not one.
///
/// `None` covers the ordinary cases and must stay cheap: `save`, `flush`, a `default` helper,
/// anything without a subject prefix. A subject with no `By` at all is still a derived query
/// (`findAll`, `countDistinct`) — it simply has no predicate.
pub fn parse(name: &str) -> Option<DerivedQuery> {
    let (prefix, subject) = SUBJECTS.iter().find(|(p, _)| name.starts_with(p))?;
    let distinct = prefix.ends_with("Distinct");
    let mut rest = &name[prefix.len()..];

    // `Top10` / `First5` / bare `Top` (meaning one).
    let mut limit = None;
    for marker in ["Top", "First"] {
        if let Some(after) = rest.strip_prefix(marker) {
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            limit = Some(digits.parse().unwrap_or(1));
            rest = &after[digits.len()..];
            break;
        }
    }

    // Everything before `By` is subject decoration Spring ignores (`findAllBy…`).
    let predicate_text = match rest.find("By") {
        Some(i) => &rest[i + 2..],
        None => {
            // No predicate at all. It is still a derived query — but only if what is left is
            // decoration, not a word we failed to understand.
            if !rest.is_empty() && rest != "All" {
                return None;
            }
            ""
        }
    };

    let (predicate_text, order_text) = match predicate_text.find("OrderBy") {
        Some(i) => (&predicate_text[..i], &predicate_text[i + 7..]),
        None => (predicate_text, ""),
    };

    Some(DerivedQuery {
        subject: *subject,
        distinct,
        limit,
        predicates: parse_predicates(predicate_text),
        order_by: parse_order(order_text),
    })
}

fn parse_predicates(text: &str) -> Vec<Predicate> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (i, (segment, or)) in split_conjunctions(text).into_iter().enumerate() {
        let (segment, ignore_case) = strip_suffix_flag(&segment, "IgnoreCase");
        let (property, keyword, args) = split_keyword(&segment);
        out.push(Predicate {
            raw: segment.clone(),
            // Filled in by `resolve`; parsing alone cannot know the entity.
            path: vec![decapitalize(&property)],
            keyword,
            args,
            ignore_case,
            or: or && i > 0,
        });
    }
    out
}

/// Split on `And` / `Or`, remembering which joined each part.
///
/// Naively splitting on the substring would break `findByBrandOrigin`, where `Or` is inside a
/// property name — so a separator only counts when the character after it starts a new
/// capitalised word, which is the same shape Spring's own tokenizer relies on.
fn split_conjunctions(text: &str) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut or_next = false;
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        for (word, is_or) in [("And", false), ("Or", true)] {
            if i > start
                && text[i..].starts_with(word)
                && text[i + word.len()..].starts_with(char::is_uppercase)
            {
                parts.push((text[start..i].to_string(), or_next));
                or_next = is_or;
                start = i + word.len();
                i = start;
                break;
            }
        }
        i += 1;
    }
    parts.push((text[start..].to_string(), or_next));
    parts.into_iter().filter(|(s, _)| !s.is_empty()).collect()
}

fn strip_suffix_flag(segment: &str, flag: &str) -> (String, bool) {
    match segment.strip_suffix(flag) {
        Some(rest) if !rest.is_empty() => (rest.to_string(), true),
        _ => (segment.to_string(), false),
    }
}

/// Split a segment into its property part and its keyword. Longest keyword wins.
fn split_keyword(segment: &str) -> (String, String, usize) {
    for (keyword, args) in KEYWORDS {
        if let Some(property) = segment.strip_suffix(keyword) {
            // A keyword IS the whole segment only in nonsense like `findByIsNull`; leaving the
            // property empty there is honest and the resolver reports it.
            return (property.to_string(), (*keyword).to_string(), *args);
        }
    }
    (segment.to_string(), String::new(), 1)
}

fn parse_order(text: &str) -> Vec<OrderTerm> {
    if text.is_empty() {
        return Vec::new();
    }
    split_conjunctions(text)
        .into_iter()
        .map(|(segment, _)| {
            let (raw, descending) = match (segment.strip_suffix("Desc"), segment.strip_suffix("Asc"))
            {
                (Some(r), _) if !r.is_empty() => (r.to_string(), true),
                (_, Some(r)) if !r.is_empty() => (r.to_string(), false),
                _ => (segment.clone(), false),
            };
            OrderTerm { path: vec![decapitalize(&raw)], raw, descending }
        })
        .collect()
}

/// Resolve every property path in `query` against `entity`, filling the paths in and reporting
/// the segments that address nothing.
///
/// Returns the resolved query alongside its issues, because the caller wants both: the paths
/// for the hover card, the issues for the squiggles.
pub fn resolve<'a>(
    model: &'a JpaModel,
    entity: &'a Entity,
    query: &DerivedQuery,
) -> (DerivedQuery, Vec<Issue>) {
    let mut out = query.clone();
    let mut issues = Vec::new();
    // The gate that keeps this honest: with an unresolved link in the inheritance chain we
    // cannot know the full field set, so nothing is reported at all.
    let verifiable = chain_is_complete(model, entity);

    for p in &mut out.predicates {
        match resolve_path(model, entity, &capitalize(&p.path[0])) {
            Some(path) => p.path = path,
            None if verifiable => issues.push(Issue {
                segment: p.raw.clone(),
                message: format!(
                    "`{}` is not a property of {} — Spring Data resolves this name at startup, \
                     so this fails when the context is built, not here.",
                    p.path[0], entity.simple,
                ),
            }),
            None => {}
        }
    }
    for o in &mut out.order_by {
        match resolve_path(model, entity, &o.raw) {
            Some(path) => o.path = path,
            None if verifiable => issues.push(Issue {
                segment: o.raw.clone(),
                message: format!("`{}` is not a property of {}.", o.path[0], entity.simple),
            }),
            None => {}
        }
    }
    (out, issues)
}

/// Whether every `@MappedSuperclass` / superclass link of `entity` resolves inside the project.
/// A chain that leaves the project means an unknown field set, and therefore no reporting.
fn chain_is_complete<'a>(model: &'a JpaModel, entity: &'a Entity) -> bool {
    let mut current = entity;
    for _ in 0..8 {
        if current.extends.is_empty() {
            return true;
        }
        // A framework base class (`AbstractPersistable`, a jar's `BaseEntity`) is exactly the
        // case that must silence the check rather than produce forty false positives.
        let Some(parent) = model.entity(&current.extends) else { return false };
        if parent.fqcn == current.fqcn {
            return true;
        }
        current = parent;
    }
    true
}

/// Resolve a camel-cased segment to a property path, following relations.
///
/// Greedy like Spring's own: the whole segment is tried as one property before any split, so a
/// literal `customerName` field wins over a `customer.name` traversal when both exist.
fn resolve_path<'a>(model: &'a JpaModel, entity: &'a Entity, camel: &str) -> Option<Vec<String>> {
    let fields = model.fields_of(entity);
    let whole = decapitalize(camel);
    if fields.iter().any(|f| f.name == whole && !f.transient) {
        return Some(vec![whole]);
    }
    // Then splits, longest head first — the same order that makes the greedy rule hold.
    for split in uppercase_boundaries(camel).into_iter().rev() {
        let head = decapitalize(&camel[..split]);
        let Some(field) = fields.iter().find(|f| f.name == head && !f.transient) else { continue };
        let target = if field.relation.is_empty() {
            crate::model::strip_generics(&field.type_text)
        } else {
            field.target.clone()
        };
        // Not a type we scanned — an `@Embedded` from a jar, a relation outside the project.
        // Unverifiable, not wrong.
        let Some(next) = model.entity(&target) else { continue };
        if next.fqcn == entity.fqcn {
            continue;
        }
        if let Some(mut rest) = resolve_path(model, next, &camel[split..]) {
            let mut path = vec![head];
            path.append(&mut rest);
            return Some(path);
        }
    }
    None
}

/// Indices where a new capitalised word starts (excluding 0).
fn uppercase_boundaries(camel: &str) -> Vec<usize> {
    camel
        .char_indices()
        .filter(|(i, c)| *i > 0 && c.is_uppercase())
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Entity, EntityField};

    fn field(name: &str, ty: &str, relation: &str, target: &str) -> EntityField {
        EntityField {
            name: name.to_string(),
            type_text: ty.to_string(),
            relation: relation.to_string(),
            target: target.to_string(),
            ..EntityField::default()
        }
    }

    /// `Order` with a `customer` relation to `Customer`, which has `name` and `city`.
    fn model() -> JpaModel {
        JpaModel {
            entities: vec![
                Entity {
                    fqcn: "com.acme.Order".into(),
                    simple: "Order".into(),
                    entity_name: "Order".into(),
                    fields: vec![
                        field("id", "Long", "", ""),
                        field("total", "BigDecimal", "", ""),
                        field("customer", "Customer", "ManyToOne", "Customer"),
                        field("brandOrigin", "String", "", ""),
                        field("scratch", "String", "", ""),
                    ],
                    ..Entity::default()
                },
                Entity {
                    fqcn: "com.acme.Customer".into(),
                    simple: "Customer".into(),
                    entity_name: "Customer".into(),
                    fields: vec![field("name", "String", "", ""), field("city", "String", "", "")],
                    ..Entity::default()
                },
            ],
            ..JpaModel::default()
        }
    }

    fn resolved(name: &str) -> (DerivedQuery, Vec<Issue>) {
        let m = model();
        let q = parse(name).expect("a derived name");
        let e = m.entity("Order").unwrap().clone();
        resolve(&m, &e, &q)
    }

    #[test]
    fn a_plain_finder_parses_its_subject_and_property() {
        let q = parse("findByTotal").unwrap();
        assert_eq!(q.subject, Subject::Find);
        assert_eq!(q.predicates.len(), 1);
        assert_eq!(q.predicates[0].path, ["total"]);
        assert_eq!(q.expected_args(), 1);
    }

    #[test]
    fn the_longest_keyword_wins() {
        // The bug this ordering prevents: `GreaterThanEqual` read as `GreaterThan` plus a
        // property called `Equal`.
        let q = parse("findByTotalGreaterThanEqual").unwrap();
        assert_eq!(q.predicates[0].keyword, "GreaterThanEqual");
        assert_eq!(q.predicates[0].path, ["total"]);
    }

    #[test]
    fn keyword_arity_is_what_makes_the_count_checkable() {
        assert_eq!(parse("findByTotalBetween").unwrap().expected_args(), 2);
        assert_eq!(parse("findByTotalIsNull").unwrap().expected_args(), 0);
        assert_eq!(parse("findByTotalIsNullAndIdIsNotNull").unwrap().expected_args(), 0);
        assert_eq!(parse("findByTotalBetweenAndId").unwrap().expected_args(), 3);
    }

    /// A relation is followed: `CustomerName` is `customer.name`.
    #[test]
    fn a_path_walks_through_a_relation() {
        let (q, issues) = resolved("findByCustomerName");
        assert_eq!(q.predicates[0].path, ["customer", "name"]);
        assert!(issues.is_empty());
    }

    /// Spring is greedy and so is this: a literal field wins over a traversal.
    #[test]
    fn a_whole_property_beats_a_split_that_would_also_work() {
        let mut m = model();
        m.entities[0].fields.push(field("customerName", "String", "", ""));
        let q = parse("findByCustomerName").unwrap();
        let e = m.entity("Order").unwrap().clone();
        let (resolved, _) = resolve(&m, &e, &q);
        assert_eq!(resolved.predicates[0].path, ["customerName"], "the field itself, not the walk");
    }

    #[test]
    fn a_typo_is_reported_in_the_terms_it_will_fail_in() {
        let (_, issues) = resolved("findByCustmerName");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].message.contains("not a property of Order"));
        assert!(issues[0].message.contains("startup"), "the user needs to know WHEN it breaks");
    }

    /// The separator rule: `Or` inside a property name is not a disjunction.
    #[test]
    fn a_property_containing_or_is_not_split_on_it() {
        let (q, issues) = resolved("findByBrandOrigin");
        assert_eq!(q.predicates.len(), 1, "one condition, not `Brand` or `igin`");
        assert_eq!(q.predicates[0].path, ["brandOrigin"]);
        assert!(issues.is_empty());
    }

    #[test]
    fn and_or_and_order_by_all_parse() {
        let (q, issues) = resolved("findByTotalGreaterThanOrCustomerCityOrderByTotalDesc");
        assert_eq!(q.predicates.len(), 2);
        assert!(q.predicates[1].or);
        assert_eq!(q.predicates[1].path, ["customer", "city"]);
        assert_eq!(q.order_by.len(), 1);
        assert_eq!(q.order_by[0].path, ["total"]);
        assert!(q.order_by[0].descending);
        assert!(issues.is_empty());
    }

    #[test]
    fn distinct_and_limits_are_read() {
        let q = parse("findDistinctTop10ByTotal").unwrap();
        assert!(q.distinct);
        assert_eq!(q.limit, Some(10));
        let q = parse("findFirstByTotal").unwrap();
        assert_eq!(q.limit, Some(1), "a bare First means one");
    }

    #[test]
    fn ignore_case_is_a_flag_not_a_property() {
        let (q, issues) = resolved("findByCustomerNameIgnoreCase");
        assert!(q.predicates[0].ignore_case);
        assert_eq!(q.predicates[0].path, ["customer", "name"]);
        assert!(issues.is_empty());
    }

    #[test]
    fn a_predicate_less_query_is_still_a_derived_query() {
        assert!(parse("findAll").is_some());
        assert_eq!(parse("findAll").unwrap().predicates.len(), 0);
        assert!(parse("countDistinct").is_some());
    }

    #[test]
    fn a_method_that_is_not_a_query_parses_as_nothing() {
        assert!(parse("save").is_none());
        assert!(parse("flush").is_none());
        assert!(parse("toString").is_none());
        assert!(parse("findSomethingWeird").is_none(), "no By, and not decoration");
    }

    /// The gate that decides whether people leave the check on: a base class outside the
    /// project means an unknown field set, so nothing is claimed.
    #[test]
    fn an_entity_whose_superclass_left_the_project_is_not_checked_at_all() {
        let mut m = model();
        m.entities[0].extends = "AbstractPersistable".to_string(); // in a jar
        let q = parse("findByAuditedAt").unwrap();
        let e = m.entity("Order").unwrap().clone();
        let (_, issues) = resolve(&m, &e, &q);
        assert!(issues.is_empty(), "unverifiable is not the same as wrong");
    }

    #[test]
    fn a_transient_field_is_not_addressable() {
        let mut m = model();
        m.entities[0].fields[4].transient = true;
        let q = parse("findByScratch").unwrap();
        let e = m.entity("Order").unwrap().clone();
        let (_, issues) = resolve(&m, &e, &q);
        assert_eq!(issues.len(), 1, "@Transient is mapped by nothing");
    }

    #[test]
    fn the_description_reads_as_a_sentence() {
        let (q, _) = resolved("findByTotalGreaterThanAndCustomerCityOrderByTotalDesc");
        assert_eq!(
            q.describe(),
            "find where total greater than and customer.city ordered by total desc",
        );
    }
}
