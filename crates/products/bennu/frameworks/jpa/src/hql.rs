//! The query text inside a `@Query` — tokenized, so it stops being a string.
//!
//! To Java, `@Query("select o from Order o where o.total > :min")` is one opaque string literal,
//! coloured like any other. That is exactly why a mistake in it survives review: a misspelled
//! parameter, a stray comma, an entity name that no longer exists all read as ordinary prose.
//!
//! Two languages share this module because they share the shape of the problem: **JPQL** (what
//! the provider parses, addressing *entities and fields*) and **native SQL** (what the database
//! parses, addressing *tables and columns*). They get different keyword sets and — more
//! importantly — different meanings, which is why `nativeQuery = true` is carried all the way
//! through rather than flattened away: an identifier in a JPQL query is checkable against the
//! entity model, and the same identifier in a native one is not checkable at all here.

/// A placeholder in a query, with its span relative to the query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    /// `:min` → `min`; `?1` → `1`.
    pub name: String,
    /// Whether it is positional (`?1`) rather than named (`:min`).
    pub positional: bool,
    pub start: usize,
    pub end: usize,
}

/// A coloured span inside the query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    /// `keyword` | `param` | `string` | `number` | `function`.
    pub kind: &'static str,
}

/// JPQL keywords. Deliberately a closed set — an unknown word is left uncoloured rather than
/// guessed at, since half of what appears in a query is the project's own vocabulary.
const JPQL: &[&str] = &[
    "select", "from", "where", "order", "by", "group", "having", "join", "left", "right", "inner",
    "outer", "fetch", "distinct", "new", "as", "and", "or", "not", "in", "like", "between", "is",
    "null", "empty", "member", "of", "exists", "all", "any", "some", "asc", "desc", "update", "set",
    "delete", "insert", "into", "values", "case", "when", "then", "else", "end", "on", "with",
    "true", "false", "count", "sum", "avg", "min", "max", "trim", "lower", "upper", "length",
    "concat", "substring", "coalesce", "nullif", "size", "index", "type", "treat", "function",
];

/// What native SQL adds on top. Kept separate so a JPQL query is never coloured with a word the
/// provider would reject.
const SQL_EXTRA: &[&str] = &[
    "table", "inner", "cross", "union", "limit", "offset", "top", "over", "partition", "rownum",
    "dual", "returning", "using", "natural", "full", "cast", "decode", "nvl", "sysdate", "current_date",
    "current_timestamp", "interval", "extract", "to_char", "to_date", "to_number", "value",
];

/// The placeholders `text` uses, in order of first appearance, deduplicated.
///
/// Placeholders inside string literals are **not** placeholders — `where note like ':min%'` binds
/// nothing, and a check built on the naive scan would demand a parameter that must not exist.
pub fn placeholders(text: &str) -> Vec<Placeholder> {
    let mut out: Vec<Placeholder> = Vec::new();
    for (start, end, kind) in scan(text) {
        if kind != "param" {
            continue;
        }
        let raw = &text[start..end];
        let positional = raw.starts_with('?');
        let name = raw[1..].to_string();
        if out.iter().any(|p| p.name == name && p.positional == positional) {
            continue;
        }
        out.push(Placeholder { name, positional, start, end });
    }
    out
}

/// Words after which the NEXT one is a name the project chose, not part of the language.
///
/// Without this, an entity called `Order` — or a table called `Group`, or `Case`, or `Set` — is
/// coloured as the keyword it collides with, which is how a perfectly good query comes to read as
/// broken. The position says which it is: whatever follows `from` is being named, whatever
/// follows nothing in particular is being used.
const NAME_INTRODUCERS: &[&str] = &["from", "join", "update", "into", "new", "table"];

/// Colouring for the query text.
pub fn tokens(text: &str, native: bool) -> Vec<Token> {
    let mut out = Vec::new();
    // The previous WORD, which is what decides whether this one is a name. Reset by anything that
    // is not a word, so a string or a parameter breaks the pairing rather than reaching across it.
    let mut previous: Option<String> = None;
    for (start, end, kind) in scan(text) {
        let kind = match kind {
            "word" => {
                let word = text[start..end].to_ascii_lowercase();
                let introduced =
                    previous.as_deref().is_some_and(|p| NAME_INTRODUCERS.contains(&p));
                previous = Some(word.clone());
                if introduced {
                    continue; // the project's own vocabulary
                }
                let known =
                    JPQL.contains(&word.as_str()) || (native && SQL_EXTRA.contains(&word.as_str()));
                if !known {
                    continue;
                }
                "keyword"
            }
            other => {
                previous = None;
                other
            }
        };
        out.push(Token { start, end, kind });
    }
    out
}

/// One pass over the text, yielding `(start, end, kind)` for the lexical pieces that matter.
///
/// Written as a single scanner rather than three passes because the only tricky part — that a
/// placeholder inside a string is not a placeholder — is precisely what a second pass would get
/// wrong. Everything else falls out of doing it once.
fn scan(text: &str) -> Vec<(usize, usize, &'static str)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            // A quoted literal — swallowed whole. `''` is an escaped quote in both languages.
            b'\'' | b'"' => {
                let quote = c;
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == quote {
                        if i + 1 < b.len() && b[i + 1] == quote {
                            i += 2;
                            continue;
                        }
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                out.push((start, i, "string"));
            }
            // Postgres's cast operator, consumed as a UNIT. Stepping over only the first colon
            // left the second one looking exactly like a named parameter, so `x::text` bound a
            // `:text` nobody wrote — and the check then demanded an argument for it.
            b':' if b.get(i + 1) == Some(&b':') => i += 2,
            // A named parameter.
            b':' => {
                let start = i;
                i += 1;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if i > start + 1 {
                    out.push((start, i, "param"));
                }
            }
            b'?' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                // A bare `?` is a JDBC placeholder with no index — legal in native SQL, and not
                // something anything below can bind by name, so it is not reported as one.
                if i > start + 1 {
                    out.push((start, i, "param"));
                }
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                out.push((start, i, "number"));
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push((start, i, "word"));
            }
            _ => i += 1,
        }
    }
    out
}

/// The entity name a JPQL query selects from, when it is written plainly enough to read.
///
/// Only the simple `from <Name>` shape — which is nearly all of them — and deliberately nothing
/// cleverer: a subquery, a join chain or a `treat(...)` is where a naive reading starts naming
/// the wrong type, and a wrong go-to target is worse than none.
pub fn from_entity(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let at = words.iter().position(|w| w.eq_ignore_ascii_case("from"))?;
    let candidate = words.get(at + 1)?;
    let name = candidate.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '.');
    (!name.is_empty() && name.starts_with(char::is_uppercase)).then(|| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(text: &str) -> Vec<String> {
        placeholders(text).into_iter().map(|p| p.name).collect()
    }

    #[test]
    fn named_and_positional_placeholders_are_both_found() {
        assert_eq!(names("where a = :min and b = ?1"), ["min", "1"]);
        let p = &placeholders("where a = :min")[0];
        assert!(!p.positional);
        assert_eq!(&"where a = :min"[p.start..p.end], ":min");
    }

    /// The reason the scan is one pass: a colon inside a literal binds nothing, and demanding a
    /// parameter for it would be a false positive on a perfectly good query.
    #[test]
    fn a_placeholder_inside_a_string_literal_is_not_one() {
        assert!(names("where note like ':min%'").is_empty());
        assert_eq!(names("where note like '%:x%' and a = :real"), ["real"]);
        // An escaped quote must not end the literal early.
        assert!(names("where s = 'it''s :fake'").is_empty());
    }

    #[test]
    fn postgres_casts_are_not_placeholders() {
        assert!(names("where id = x::text").is_empty());
    }

    #[test]
    fn a_repeated_placeholder_is_reported_once() {
        assert_eq!(names("where a = :x or b = :x"), ["x"]);
    }

    #[test]
    fn a_bare_jdbc_question_mark_is_not_a_named_binding() {
        assert!(names("where a = ?").is_empty());
        assert_eq!(names("where a = ?1"), ["1"]);
    }

    #[test]
    fn keywords_colour_and_project_vocabulary_does_not() {
        let q = "select o from Order o where o.total > :min";
        let kinds: Vec<(&str, &str)> =
            tokens(q, false).iter().map(|t| (&q[t.start..t.end], t.kind)).collect();
        assert!(kinds.contains(&("select", "keyword")));
        assert!(kinds.contains(&("from", "keyword")));
        assert!(kinds.contains(&(":min", "param")));
        assert!(!kinds.iter().any(|(w, _)| *w == "Order"), "the project's own vocabulary");
    }

    /// The two languages are not coloured the same, which is the visible half of carrying
    /// `nativeQuery` through instead of flattening it.
    #[test]
    fn native_only_keywords_colour_only_in_a_native_query() {
        let q = "select * from ORDERS limit 10";
        let word = |native| {
            tokens(q, native).iter().any(|t| &q[t.start..t.end] == "limit" && t.kind == "keyword")
        };
        assert!(word(true));
        assert!(!word(false), "`limit` is not JPQL — colouring it would say it is");
    }

    #[test]
    fn the_selected_entity_is_read_from_the_plain_shape_only() {
        assert_eq!(from_entity("select o from Order o where o.id = :id").as_deref(), Some("Order"));
        assert_eq!(from_entity("SELECT o FROM Order o").as_deref(), Some("Order"));
        // A native query selects from a table, which is not an entity name.
        assert_eq!(from_entity("select * from orders"), None);
        assert_eq!(from_entity("select count(*) from"), None);
    }
}
