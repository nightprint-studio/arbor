//! The search bar's whole contract: `type:bug stato:aperto testo libero`.
//!
//! [`parse_query`] is total. There is no error path and no `Result`: anything it
//! cannot read as a filter becomes free text. A search box that refuses to
//! search because you typed `12:30` would be worse than useless, and the user
//! finds out something was not a filter by looking at the results, immediately.
//!
//! Grammar, informally:
//!
//! ```text
//! query   := token*
//! token   := '#' tag | key ':' value | word | '"' words '"'
//! key     := letter (letter | digit | '_' | '-' | '.')*
//! value   := op? (word | '"' words '"')
//! op      := '!' | '~' | '>' | '>=' | '<' | '<='
//! ```
//!
//! `type`, `tag` and `sort` are reserved; **every other key is a frontmatter
//! field filter**, because the query language cannot know a vault's types and
//! must not reject `stato:aperto` for being unfamiliar.

use garrulus_vault::prelude::TypeId;
use serde::{Deserialize, Serialize};

use crate::note_view::{type_id, NoteView};

/// How a field filter compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterOp {
    /// `key:value` — equal, ignoring case.
    Eq,
    /// `key:!value` — not equal.
    Ne,
    /// `key:~value` — substring, ignoring case.
    Contains,
    /// `key:>value`
    Gt,
    /// `key:>=value`
    Gte,
    /// `key:<value`
    Lt,
    /// `key:<=value`
    Lte,
}

/// One structured constraint on a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Filter {
    /// `type:bug`
    Type(TypeId),
    /// `tag:sync` or `#sync`
    Tag(String),
    /// Any other `key:value`, compared against flattened frontmatter.
    Field {
        /// Frontmatter key, lowercased.
        key: String,
        /// Comparison to apply.
        op: FilterOp,
        /// Right-hand side, as written (minus quotes and operator).
        value: String,
    },
}

/// What a result list is ordered by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortField {
    /// Note title.
    Title,
    /// Filesystem mtime — resolved by the caller, not by the index.
    Modified,
    /// Creation time — resolved by the caller, not by the index.
    Created,
    /// A frontmatter field.
    Field(String),
}

/// Ascending or descending.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    /// Smallest first (the default).
    Asc,
    /// Largest first — written `sort:-title`.
    Desc,
}

/// A parsed `sort:` term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortKey {
    /// What to order by.
    pub field: SortField,
    /// Which way.
    pub order: SortOrder,
}

/// A parsed search bar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Query {
    /// Everything that was not a filter, space-joined. `None` when empty.
    pub text: Option<String>,
    /// Structured constraints, in the order they were typed.
    pub filters: Vec<Filter>,
    /// The last `sort:` term seen, if any.
    pub sort: Option<SortKey>,
}

impl Query {
    /// A query with nothing but free text.
    pub fn from_text(text: impl Into<String>) -> Self {
        Self { text: Some(text.into()), ..Self::default() }
    }

    /// Whether this query constrains nothing at all.
    pub fn is_empty(&self) -> bool {
        self.text.is_none() && self.filters.is_empty()
    }
}

/// Parse a search bar. Never fails; unreadable tokens degrade to free text.
pub fn parse_query(input: &str) -> Query {
    let mut query = Query::default();
    let mut words: Vec<String> = Vec::new();

    for token in split_tokens(input) {
        match classify(&token) {
            Token::Sort(key) => query.sort = Some(key),
            Token::Filter(f) => query.filters.push(f),
            Token::Text(t) => words.push(t),
        }
    }

    if !words.is_empty() {
        query.text = Some(words.join(" "));
    }
    query
}

/// What one token turned out to be.
enum Token {
    Sort(SortKey),
    Filter(Filter),
    Text(String),
}

/// Split on whitespace, keeping `"quoted runs"` together. The quotes survive
/// into the token so that a token that fails to be a filter can be un-quoted
/// once, at the end, instead of twice on two paths.
fn split_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;

    for c in input.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                current.push(c);
            }
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                current.push(c);
            }
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            None => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn classify(token: &str) -> Token {
    if let Some(tag) = token.strip_prefix('#') {
        let tag = unquote(tag);
        if !tag.is_empty() {
            return Token::Filter(Filter::Tag(tag));
        }
        return Token::Text(unquote(token));
    }

    let Some((key, rest)) = token.split_once(':') else {
        return Token::Text(unquote(token));
    };
    let key = key.to_lowercase();
    let (op, value) = split_op(rest);
    let value = unquote(value);

    // `12:30` (key is not an identifier), `sort:` (no value) and
    // `https://example.com` (a scheme, not a key) all look like filters and are
    // not: they go back to being text rather than becoming nonsense constraints.
    if !is_key(&key) || value.is_empty() || value.starts_with("//") {
        return Token::Text(unquote(token));
    }

    match key.as_str() {
        "sort" => Token::Sort(parse_sort(&value)),
        "type" => Token::Filter(Filter::Type(type_id(&value))),
        "tag" => Token::Filter(Filter::Tag(value)),
        _ => Token::Filter(Filter::Field { key, op, value }),
    }
}

/// A key must look like an identifier; `12` and `https` followed by `//` do not.
fn is_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Peel a leading comparison operator off a filter value.
fn split_op(value: &str) -> (FilterOp, &str) {
    for (prefix, op) in [
        (">=", FilterOp::Gte),
        ("<=", FilterOp::Lte),
        ("!", FilterOp::Ne),
        ("~", FilterOp::Contains),
        (">", FilterOp::Gt),
        ("<", FilterOp::Lt),
    ] {
        if let Some(rest) = value.strip_prefix(prefix) {
            return (op, rest);
        }
    }
    (FilterOp::Eq, value)
}

/// Drop one matching pair of surrounding quotes.
fn unquote(s: &str) -> String {
    for q in ['"', '\''] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            return s[1..s.len() - 1].to_owned();
        }
    }
    s.to_owned()
}

fn parse_sort(value: &str) -> SortKey {
    let (order, name) = match value.strip_prefix('-') {
        Some(rest) => (SortOrder::Desc, rest),
        None => (SortOrder::Asc, value),
    };
    let field = match name.to_lowercase().as_str() {
        "title" | "titolo" => SortField::Title,
        "modified" | "modificato" => SortField::Modified,
        "created" | "creato" => SortField::Created,
        other => SortField::Field(other.to_owned()),
    };
    SortKey { field, order }
}

/// Whether a note satisfies every filter in `query`.
pub fn matches_filters(view: &NoteView, filters: &[Filter]) -> bool {
    filters.iter().all(|f| matches_filter(view, f))
}

/// Whether a note satisfies one filter.
pub fn matches_filter(view: &NoteView, filter: &Filter) -> bool {
    match filter {
        Filter::Type(t) => view.kind.as_ref() == Some(t),
        Filter::Tag(tag) => view.has_tag(tag),
        Filter::Field { key, op, value } => {
            let actual = view.fields.get(key).map(String::as_str);
            compare(actual, *op, value)
        }
    }
}

/// Apply one comparison. A missing field only ever satisfies `!=`, so
/// `stato:!chiuso` includes the notes that have no `stato` at all — which is
/// what "non chiuso" means to a human.
fn compare(actual: Option<&str>, op: FilterOp, expected: &str) -> bool {
    let Some(actual) = actual else {
        return op == FilterOp::Ne;
    };
    match op {
        FilterOp::Eq => actual.eq_ignore_ascii_case(expected),
        FilterOp::Ne => !actual.eq_ignore_ascii_case(expected),
        FilterOp::Contains => actual.to_lowercase().contains(&expected.to_lowercase()),
        FilterOp::Gt | FilterOp::Gte | FilterOp::Lt | FilterOp::Lte => {
            let ordering = match (actual.parse::<f64>(), expected.parse::<f64>()) {
                // Numeric when both sides are numbers, so `priority:>2` does not
                // decide that "10" < "2"; lexicographic otherwise, which is what
                // ISO dates need.
                (Ok(a), Ok(b)) => a.partial_cmp(&b),
                _ => Some(actual.to_lowercase().cmp(&expected.to_lowercase())),
            };
            match (op, ordering) {
                (_, None) => false,
                (FilterOp::Gt, Some(o)) => o.is_gt(),
                (FilterOp::Gte, Some(o)) => o.is_ge(),
                (FilterOp::Lt, Some(o)) => o.is_lt(),
                (FilterOp::Lte, Some(o)) => o.is_le(),
                _ => unreachable!("only ordering ops reach here"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note_view::note_id;

    fn field(key: &str, op: FilterOp, value: &str) -> Filter {
        Filter::Field { key: key.into(), op, value: value.into() }
    }

    #[test]
    fn bare_text_is_only_text() {
        let q = parse_query("appunti di riunione");
        assert_eq!(q.text.as_deref(), Some("appunti di riunione"));
        assert!(q.filters.is_empty());
        assert!(q.sort.is_none());
    }

    #[test]
    fn an_empty_input_is_an_empty_query() {
        let q = parse_query("   ");
        assert_eq!(q, Query::default());
        assert!(q.is_empty());
    }

    #[test]
    fn a_single_reserved_filter() {
        let q = parse_query("type:bug");
        assert_eq!(q.filters, vec![Filter::Type(type_id("bug"))]);
        assert!(q.text.is_none());
    }

    #[test]
    fn several_filters_plus_free_text_in_any_order() {
        let q = parse_query("type:bug stato:aperto testo libero");
        assert_eq!(
            q.filters,
            vec![Filter::Type(type_id("bug")), field("stato", FilterOp::Eq, "aperto")]
        );
        assert_eq!(q.text.as_deref(), Some("testo libero"));

        let scrambled = parse_query("testo type:bug libero stato:aperto");
        assert_eq!(scrambled.filters, q.filters);
        assert_eq!(scrambled.text.as_deref(), Some("testo libero"));
    }

    #[test]
    fn both_tag_spellings_produce_the_same_filter() {
        assert_eq!(parse_query("#sync").filters, vec![Filter::Tag("sync".into())]);
        assert_eq!(parse_query("tag:sync").filters, vec![Filter::Tag("sync".into())]);
    }

    #[test]
    fn quoted_values_keep_their_spaces() {
        let q = parse_query(r#"stato:"in corso" progetto:'due parole'"#);
        assert_eq!(
            q.filters,
            vec![
                field("stato", FilterOp::Eq, "in corso"),
                field("progetto", FilterOp::Eq, "due parole"),
            ]
        );
        assert!(q.text.is_none());
    }

    #[test]
    fn quoted_free_text_survives_as_one_phrase() {
        let q = parse_query(r#""nota di lavoro" type:bug"#);
        assert_eq!(q.text.as_deref(), Some("nota di lavoro"));
        assert_eq!(q.filters, vec![Filter::Type(type_id("bug"))]);
    }

    #[test]
    fn operators_are_peeled_off_the_value() {
        let q = parse_query("priority:>=2 stato:!chiuso titolo:~sync scadenza:<2026-01-01");
        assert_eq!(
            q.filters,
            vec![
                field("priority", FilterOp::Gte, "2"),
                field("stato", FilterOp::Ne, "chiuso"),
                field("titolo", FilterOp::Contains, "sync"),
                field("scadenza", FilterOp::Lt, "2026-01-01"),
            ]
        );
    }

    #[test]
    fn keys_are_lowercased_but_values_are_not() {
        let q = parse_query("Stato:Aperto");
        assert_eq!(q.filters, vec![field("stato", FilterOp::Eq, "Aperto")]);
    }

    #[test]
    fn an_unusable_key_degrades_to_text_and_never_errors() {
        for input in ["12:30", "https://example.com", "sort:", "type:", ":vuoto", "a b:"] {
            let q = parse_query(input);
            assert!(q.filters.is_empty(), "{input:?} produced filters: {:?}", q.filters);
            assert!(q.sort.is_none(), "{input:?} produced a sort key");
            assert!(q.text.is_some(), "{input:?} produced no text either");
        }
        assert_eq!(parse_query("12:30 riunione").text.as_deref(), Some("12:30 riunione"));
        assert_eq!(
            parse_query("https://example.com").text.as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn an_unknown_key_is_a_field_filter_not_an_error() {
        // The query language has no type registry, so it cannot tell a typo from
        // a legitimate frontmatter key. It must not reject either.
        let q = parse_query("chiavemaiVista:qualcosa");
        assert_eq!(q.filters, vec![field("chiavemaivista", FilterOp::Eq, "qualcosa")]);
    }

    #[test]
    fn sort_parses_direction_and_falls_back_to_a_field() {
        assert_eq!(
            parse_query("sort:title").sort,
            Some(SortKey { field: SortField::Title, order: SortOrder::Asc })
        );
        assert_eq!(
            parse_query("sort:-modified").sort,
            Some(SortKey { field: SortField::Modified, order: SortOrder::Desc })
        );
        assert_eq!(
            parse_query("sort:severity").sort,
            Some(SortKey { field: SortField::Field("severity".into()), order: SortOrder::Asc })
        );
        // Last one wins, and sort never leaks into the filters.
        let q = parse_query("sort:title sort:-created");
        assert_eq!(q.sort.unwrap().order, SortOrder::Desc);
        assert!(q.filters.is_empty());
    }

    fn sample() -> NoteView {
        let mut v = NoteView::new(note_id("a"), "Bug di sync");
        v.kind = Some(type_id("bug"));
        v.tags = vec!["sync".into()];
        v.fields.insert("stato".into(), "aperto".into());
        v.fields.insert("priority".into(), "3".into());
        v
    }

    #[test]
    fn filters_apply_as_a_conjunction() {
        let v = sample();
        assert!(matches_filters(&v, &parse_query("type:bug stato:aperto").filters));
        assert!(!matches_filters(&v, &parse_query("type:bug stato:chiuso").filters));
        assert!(matches_filters(&v, &parse_query("#sync").filters));
        assert!(!matches_filters(&v, &parse_query("#altro").filters));
    }

    #[test]
    fn numeric_fields_compare_as_numbers_not_as_strings() {
        let mut v = sample();
        v.fields.insert("priority".into(), "10".into());
        assert!(matches_filters(&v, &parse_query("priority:>2").filters));
        assert!(!matches_filters(&v, &parse_query("priority:<2").filters));
    }

    #[test]
    fn a_missing_field_only_satisfies_a_negation() {
        let v = sample();
        assert!(matches_filters(&v, &parse_query("assente:!qualcosa").filters));
        assert!(!matches_filters(&v, &parse_query("assente:qualcosa").filters));
        assert!(!matches_filters(&v, &parse_query("assente:~qualcosa").filters));
    }
}
