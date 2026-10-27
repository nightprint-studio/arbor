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

/// The JPQL literal the caret is inside, with its span in `source`.
pub fn query_at(source: &str, offset: usize) -> Option<QueryDef> {
    let (start, end) = literal_around(source, offset)?;
    let head = source[..start - 1].trim_end();
    let is_jpql = ends_with_query_call(head) || ends_with_named_query_value(head);
    is_jpql.then(|| QueryDef {
        text: source[start..end].to_string(),
        start,
        end,
        ..QueryDef::default()
    })
}

/// `…createQuery(` — the opening parenthesis directly before the literal.
fn ends_with_query_call(head: &str) -> bool {
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
    QUERY_CALLS.contains(&name.as_str())
}

/// `@NamedQuery(name = "…", query = ` — and not `@NamedNativeQuery`, whose `query` is SQL.
fn ends_with_named_query_value(head: &str) -> bool {
    let Some(before_eq) = head.strip_suffix('=') else { return false };
    if !before_eq.trim_end().ends_with("query") {
        return false;
    }
    // Stay inside the annotation: the nearest `@Named…` must be `@NamedQuery`, and close enough to
    // be the one this attribute belongs to.
    let window = &head[head.len().saturating_sub(600)..];
    window.rfind("@Named").is_some_and(|at| window[at..].starts_with("@NamedQuery("))
}

/// The contents span of the single-line string literal around `offset`, quotes excluded.
fn literal_around(source: &str, offset: usize) -> Option<(usize, usize)> {
    let line_start = source[..offset.min(source.len())].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[line_start..].find('\n').map_or(source.len(), |i| line_start + i);
    let line = &source[line_start..line_end];
    // A text block opens with three quotes; its lines are not literals of their own.
    if line.contains("\"\"\"") {
        return None;
    }
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                // A char literal (`'"'`, `'\''`): skip it whole so its quote does not open a string.
                i += if bytes.get(i + 1) == Some(&b'\\') { 4 } else { 3 };
            }
            b'"' => {
                let open = i + 1;
                let mut j = open;
                while j < bytes.len() && bytes[j] != b'"' {
                    j += if bytes[j] == b'\\' { 2 } else { 1 };
                }
                if j >= bytes.len() {
                    return None; // unterminated while typing: not a literal yet
                }
                let (s, e) = (line_start + open, line_start + j);
                if (s..=e).contains(&offset) {
                    return Some((s, e));
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    None
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

    #[test]
    fn a_text_block_line_is_not_read() {
        assert!(at("em.createQuery(\"\"\"\n  select o from O|\n\"\"\");").is_none());
    }
}
