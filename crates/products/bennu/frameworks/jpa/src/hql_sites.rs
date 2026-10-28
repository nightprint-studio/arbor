//! Finding a JPQL query that is not a repository `@Query` — the one in `@NamedQuery(query = "…")`
//! and the one handed to `createQuery("…")`.
//!
//! Only what can be read with certainty: a **single-line** string literal that is the argument of
//! one of those exact forms. A text block, a concatenation or a literal passed through a variable
//! is a query this module cannot see whole, and a completion offered over half of one would be
//! computed against the wrong text. `createNativeQuery` is deliberately absent — that is SQL.

use crate::model::QueryDef;

/// Calls whose first argument is a JPQL / HQL string.
const QUERY_CALLS: &[&str] = &["createQuery", "createSelectionQuery", "createMutationQuery"];

/// Calls whose first argument is a native SQL string.
const NATIVE_CALLS: &[&str] = &["createNativeQuery", "createSQLQuery"];

/// The JPQL literal the caret is inside, with its span in `source`.
pub fn query_at(source: &str, offset: usize) -> Option<QueryDef> {
    let lit = literals(source).into_iter().find(|l| (l.start..=l.end).contains(&offset))?;
    (language_of(source, lit.open) == Some(false)).then(|| QueryDef {
        text: source[lit.start..lit.end].to_string(),
        start: lit.start,
        end: lit.end,
        ..QueryDef::default()
    })
}

/// Every query string this module can read in `source` — JPQL and native alike — for colouring.
/// A string is one when the call or annotation it is the argument of says so, or when a
/// `// language=…` comment or `@Language("…")` (IntelliJ's own convention) names its language.
/// A text block (`"""…"""`) is a string like any other. Pieces of a `"a" + "b"` concatenation share
/// the language of the first.
pub fn sites(source: &str) -> Vec<QueryDef> {
    let mut out: Vec<QueryDef> = Vec::new();
    // Where the previous query string ended (after its closing delimiter), and what it was.
    let mut previous: Option<(usize, bool)> = None;
    for lit in literals(source) {
        let continued = previous
            .filter(|(close, _)| source[*close..lit.open].trim() == "+")
            .map(|(_, native)| native);
        let language = continued.or_else(|| language_of(source, lit.open));
        previous = language.map(|native| (lit.close, native));
        if let Some(native) = language {
            out.push(QueryDef {
                text: source[lit.start..lit.end].to_string(),
                start: lit.start,
                end: lit.end,
                native,
                ..QueryDef::default()
            });
        }
    }
    out
}

/// Whether the literal opening at `open` is native SQL (`Some(true)`), JPQL (`Some(false)`) or
/// neither (`None`).
fn language_of(source: &str, open: usize) -> Option<bool> {
    let head = source[..open].trim_end();
    if ends_with_call(head, QUERY_CALLS) {
        return Some(false);
    }
    if ends_with_call(head, NATIVE_CALLS) {
        return Some(true);
    }
    if let Some(native) = named_query_value(head) {
        return Some(native);
    }
    injected_language(source, open)
}

/// `// language=SQL` above the statement, `/* language=SQL */` before the literal or
/// `@Language("SQL")` on the declaration: IntelliJ's way of saying what a string holds, honoured
/// when it sits in the same statement as the literal.
fn injected_language(source: &str, open: usize) -> Option<bool> {
    let mut from = open.saturating_sub(600);
    while !source.is_char_boundary(from) {
        from += 1;
    }
    let window = &source[from..open];
    // The statement the literal is in: nothing after the last `;`, `{` or `}`.
    let stmt = &window[window.rfind([';', '{', '}']).map_or(0, |i| i + 1)..];
    let lower = stmt.to_ascii_lowercase();
    let at = lower.rfind("language")?;
    let before = &stmt[..at];
    let rest = lower[at + "language".len()..].trim_start();
    let value = if let Some(after) = rest.strip_prefix('=') {
        // `//` or `/*` on the same line makes it a comment, not code that happens to say "language".
        let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
        if !(line.contains("//") || line.contains("/*")) {
            return None;
        }
        after.trim_start().trim_start_matches('"')
    } else if before.ends_with('@') {
        let inner = rest.strip_prefix('(')?.trim_start();
        let inner = inner.strip_prefix("value").map_or(inner, |v| v.trim_start().trim_start_matches('=').trim_start());
        inner.trim_start_matches('"')
    } else {
        return None;
    };
    let word: String = value.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    if word.contains("jpa") || word.contains("hql") || word.contains("jpql") {
        Some(false)
    } else if word.contains("sql") || word == "oracle" {
        Some(true)
    } else {
        None
    }
}

/// `…createQuery(` — the opening parenthesis directly before the literal — for one of `calls`.
fn ends_with_call(head: &str, calls: &[&str]) -> bool {
    let Some(before_paren) = head.strip_suffix('(') else { return false };
    let name: String = before_paren
        .trim_end()
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    calls.contains(&name.as_str())
}

/// `@NamedQuery(name = "…", query = ` is JPQL (`Some(false)`), `@NamedNativeQuery(…)` is SQL
/// (`Some(true)`); anything else is not a named query's text.
fn named_query_value(head: &str) -> Option<bool> {
    let before_eq = head.strip_suffix('=')?;
    if !before_eq.trim_end().ends_with("query") {
        return None;
    }
    // Stay inside the annotation: the nearest `@Named…` has to be close enough to be the one this
    // attribute belongs to.
    let window = &head[head.len().saturating_sub(600)..];
    let at = window.rfind("@Named")?;
    if window[at..].starts_with("@NamedQuery(") {
        Some(false)
    } else if window[at..].starts_with("@NamedNativeQuery(") {
        Some(true)
    } else {
        None
    }
}

/// One string literal: where its delimiter opens, its contents, and where it ends.
struct Literal {
    /// Byte offset of the opening quote (of the `"""`, for a text block).
    open: usize,
    /// Contents span, delimiters excluded. A text block's starts after the line break that follows
    /// its opening `"""`.
    start: usize,
    end: usize,
    /// Just past the closing delimiter.
    close: usize,
}

/// Every string literal in `source`, text blocks included. Comments are skipped, so a quote inside
/// one does not open a string, and a literal still being typed (no closing quote) is not one yet.
fn literals(source: &str) -> Vec<Literal> {
    let b = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = source[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
            }
            b'\'' => {
                // A char literal (`'"'`, `'\''`): skip it whole so its quote does not open a string.
                i += if b.get(i + 1) == Some(&b'\\') { 4 } else { 3 };
            }
            b'"' if source[i..].starts_with("\"\"\"") => {
                let mut start = i + 3;
                // The contents begin on the line after the opening delimiter.
                let blanks = source[start..].bytes().take_while(|c| *c == b' ' || *c == b'\t').count();
                let after_blanks = start + blanks;
                if b.get(after_blanks) == Some(&b'\n') {
                    start = after_blanks + 1;
                } else if source[after_blanks..].starts_with("\r\n") {
                    start = after_blanks + 2;
                }
                let mut j = start;
                let mut closed = None;
                while j < b.len() {
                    if b[j] == b'\\' {
                        j += 2;
                    } else if b[j..].starts_with(b"\"\"\"") {
                        closed = Some(j);
                        break;
                    } else {
                        j += 1;
                    }
                }
                let Some(end) = closed else { break };
                out.push(Literal { open: i, start, end, close: end + 3 });
                i = end + 3;
            }
            b'"' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j] != b'"' && b[j] != b'\n' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                if j >= b.len() || b[j] != b'"' {
                    i = j; // unterminated on its line: not a literal yet
                    continue;
                }
                out.push(Literal { open: i, start, end: j, close: j + 1 });
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The query at the caret marked `|`.
    fn at(src: &str) -> Option<String> {
        let caret = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        query_at(&src, caret).map(|q| q.text)
    }

    #[test]
    fn a_create_query_argument_is_a_query() {
        assert_eq!(at(r#"em.createQuery("select o from O|", X.class);"#).as_deref(), Some("select o from O"));
        assert!(at(r#"em.createQuery("select o from O|");"#).is_some());
    }

    #[test]
    fn a_native_query_is_not() {
        assert!(at(r#"em.createNativeQuery("select * from t|");"#).is_none());
    }

    #[test]
    fn a_query_in_a_language_comment_is_found_by_the_caret_too() {
        assert!(at("// language=jpaql\nString q = \"select o from O|\";").is_some());
        assert!(at("// language=sql\nString q = \"select o from O|\";").is_none());
    }

    #[test]
    fn a_named_query_value_is_a_query_but_a_named_native_one_is_not() {
        assert!(at(r#"@NamedQuery(name = "A.all", query = "select a from A| a")"#).is_some());
        assert!(at(r#"@NamedNativeQuery(name = "A.all", query = "select * from A|")"#).is_none());
    }

    #[test]
    fn the_name_of_a_named_query_is_not_a_query() {
        assert!(at(r#"@NamedQuery(name = "A.al|l", query = "select a from A a")"#).is_none());
    }

    #[test]
    fn an_unrelated_string_is_left_alone() {
        assert!(at(r#"log.info("select o from O|");"#).is_none());
    }

    #[test]
    fn a_char_literal_quote_does_not_open_a_string() {
        assert!(at(r#"char q = '"'; em.createQuery("from O|");"#).is_some());
    }

    fn site_texts(src: &str) -> Vec<(String, bool)> {
        sites(src).into_iter().map(|q| (q.text, q.native)).collect()
    }

    #[test]
    fn a_language_comment_says_what_the_string_is() {
        let src = "// language=SQL\nString sql = \"select * from t\";\nString other = \"select 1\";\n";
        assert_eq!(site_texts(src), vec![("select * from t".to_string(), true)]);
        let jpql = "// language=JPAQL\nString q = \"select o from O o\";";
        assert_eq!(site_texts(jpql), vec![("select o from O o".to_string(), false)]);
    }

    #[test]
    fn an_inline_comment_and_the_language_annotation_work_too() {
        assert_eq!(site_texts("jdbc.query(/* language=sql */ \"select 1\", m);").len(), 1);
        assert_eq!(site_texts("@Language(\"SQL\") String q = \"select 1\";").len(), 1);
        assert_eq!(site_texts("@Language(value = \"PostgreSQL\") String q = \"select 1\";").len(), 1);
    }

    #[test]
    fn a_comment_in_an_earlier_statement_does_not_reach_a_later_string() {
        assert!(site_texts("// language=sql\nint a = 1;\nString s = \"select 1\";").is_empty());
    }

    #[test]
    fn a_concatenation_keeps_the_language_of_its_first_piece() {
        let src = "em.createNativeQuery(\"select a \"\n  + \"from t\");";
        assert_eq!(site_texts(src).len(), 2);
    }

    #[test]
    fn native_calls_and_named_native_queries_are_native() {
        assert_eq!(site_texts("em.createNativeQuery(\"select 1\");"), vec![("select 1".to_string(), true)]);
        assert_eq!(
            site_texts("@NamedNativeQuery(name = \"A\", query = \"select 1\")"),
            vec![("select 1".to_string(), true)]
        );
    }

    #[test]
    fn a_text_block_is_one_string() {
        let src = "em.createQuery(\"\"\"\n  select o from O|\n  \"\"\");";
        assert_eq!(at(src).unwrap().trim(), "select o from O");
    }

    #[test]
    fn a_language_comment_reaches_a_text_block() {
        let src = "// language=sql\nString q = \"\"\"\n  SELECT a\n    FROM t\n  \"\"\";\nString other = \"x\";";
        let found = sites(src);
        assert_eq!(found.len(), 1);
        assert!(found[0].native);
        assert!(found[0].text.contains("FROM t"));
    }

    #[test]
    fn a_quote_inside_a_comment_opens_nothing() {
        assert!(site_texts("// he said \"select 1\"\nint a;").is_empty());
    }
}
