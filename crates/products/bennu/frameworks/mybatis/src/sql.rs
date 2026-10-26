//! The SQL inside a statement — lexed into the pieces worth colouring.
//!
//! Written against a *slice of the buffer* (one run of character data between two tags), reporting
//! spans in the buffer's own offsets, so the caller never translates anything.
//!
//! A closed keyword set, on purpose: half of what appears in a statement is the project's own
//! vocabulary (`user`, `value`, `type`, `index` are all columns somewhere), and colouring a column
//! as a keyword makes a perfectly good query read as broken. For the same reason a word right after
//! `from` / `join` / `into` / `update` / `table`, or after a `.`, is a **name** and never a keyword.

use bennu_ext::prelude::ExtHighlight;

/// Reserved words whose meaning is the same in every dialect a mapper is likely to be written for.
const KEYWORDS: &[&str] = &[
    "select", "from", "where", "and", "or", "not", "in", "like", "ilike", "between", "is", "null",
    "join", "left", "right", "inner", "outer", "full", "cross", "natural", "on", "using", "group",
    "by", "order", "having", "limit", "offset", "fetch", "first", "next", "rows", "only", "union",
    "all", "intersect", "except", "minus", "distinct", "as", "case", "when", "then", "else", "end",
    "insert", "into", "values", "update", "set", "delete", "exists", "any", "some", "asc", "desc",
    "with", "recursive", "create", "alter", "drop", "table", "view", "truncate", "returning",
    "default", "true", "false", "for", "if", "over", "partition", "escape", "collate", "nulls",
    "merge", "matched", "lateral", "unique", "primary", "foreign", "references", "constraint",
];

/// Words that are a function call when followed by `(`. Kept apart from the keywords because
/// `left(` and `right(` are string functions in some dialects and joins in all of them.
const FUNCTIONS: &[&str] = &[
    "count", "sum", "avg", "min", "max", "coalesce", "nvl", "nvl2", "ifnull", "nullif", "decode",
    "upper", "lower", "trim", "ltrim", "rtrim", "substr", "substring", "concat", "length", "replace",
    "round", "floor", "ceil", "ceiling", "abs", "mod", "cast", "convert", "to_char", "to_date",
    "to_number", "to_timestamp", "date_format", "str_to_date", "date_add", "date_sub", "datediff",
    "now", "sysdate", "extract", "instr", "lpad", "rpad", "row_number", "rank", "dense_rank",
    "lead", "lag", "greatest", "least", "group_concat", "listagg", "string_agg", "json_extract",
];

/// Value-like words with no parentheses: coloured with the keywords, since they read the same way.
const CONSTANTS: &[&str] = &["current_date", "current_time", "current_timestamp", "sysdate", "systimestamp"];

/// After one of these, the next word is a table (or alias) the project named.
const NAME_INTRODUCERS: &[&str] = &["from", "join", "update", "into", "table"];

fn is_word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$')
}

/// Append the highlights of the SQL in `source[start..end]` to `out`.
pub(crate) fn highlight(source: &str, start: usize, end: usize, out: &mut Vec<ExtHighlight>) {
    let b = source.as_bytes();
    let mut push = |s: usize, e: usize, kind: &str| {
        if e > s {
            out.push(ExtHighlight { start: s, end: e, kind: format!("mybatis.{kind}") });
        }
    };
    let mut i = start;
    // The previous WORD, for the name-introducer rule. Anything but whitespace and comments between
    // two words breaks the pairing, so a comma or a parenthesis never lets one reach across it.
    let mut previous: Option<String> = None;
    let mut after_dot = false;

    while i < end {
        let c = b[i];
        match c {
            b'-' if b.get(i + 1) == Some(&b'-') => {
                let stop = b[i..end].iter().position(|x| *x == b'\n').map_or(end, |p| i + p);
                push(i, stop, "sql.comment");
                i = stop;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let stop = source[i + 2..end].find("*/").map_or(end, |p| i + 2 + p + 2);
                push(i, stop, "sql.comment");
                i = stop;
            }
            b'\'' => {
                i = string(b, i, end, &mut push);
                previous = None;
                after_dot = false;
            }
            b'"' | b'`' => {
                // A quoted identifier: a name, left in the markup's own colour — but a binding
                // inside it is still a binding.
                let close = b[i + 1..end].iter().position(|x| *x == c).map_or(end, |p| i + 1 + p + 1);
                bindings_in(b, i, close, &mut push);
                i = close;
                previous = None;
                after_dot = false;
            }
            b'#' | b'$' if b.get(i + 1) == Some(&b'{') => {
                i = binding(b, i, end, &mut push);
                previous = None;
                after_dot = false;
            }
            b'&' => {
                // `&lt;` `&gt;` `&amp;` — an entity, not the letters inside it.
                let stop = b[i..end.min(i + 8)].iter().position(|x| *x == b';').map_or(i + 1, |p| i + p + 1);
                i = stop;
                previous = None;
                after_dot = false;
            }
            _ if c.is_ascii_digit() => {
                let from = i;
                while i < end && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                push(from, i, "sql.number");
                previous = None;
                after_dot = false;
            }
            _ if is_word_start(c) => {
                let from = i;
                while i < end && is_word_byte(b[i]) {
                    i += 1;
                }
                let word = source[from..i].to_ascii_lowercase();
                let named = after_dot || previous.as_deref().is_some_and(|p| NAME_INTRODUCERS.contains(&p));
                if !named {
                    if let Some(kind) = classify(&word, followed_by_paren(b, i, end)) {
                        push(from, i, kind);
                    }
                }
                previous = Some(word);
                after_dot = false;
            }
            b'.' => {
                i += 1;
                after_dot = true;
            }
            _ if c.is_ascii_whitespace() => i += 1,
            _ => {
                i += 1;
                previous = None;
                after_dot = false;
            }
        }
    }
}

fn classify(word: &str, call: bool) -> Option<&'static str> {
    if call && FUNCTIONS.contains(&word) {
        Some("sql.function")
    } else if KEYWORDS.contains(&word) || CONSTANTS.contains(&word) {
        Some("sql.keyword")
    } else {
        None
    }
}

fn followed_by_paren(b: &[u8], mut i: usize, end: usize) -> bool {
    while i < end && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i < end && b[i] == b'('
}

/// A `#{…}` / `${…}` starting at `at`; returns where the scan resumes.
fn binding(b: &[u8], at: usize, end: usize, push: &mut impl FnMut(usize, usize, &str)) -> usize {
    let close = b[at..end].iter().position(|x| *x == b'}').map_or(end, |p| at + p + 1);
    // `${…}` is spliced into the statement text rather than bound, which is the one that can be an
    // injection — so it gets its own kind and the theme gets to make it look like what it is.
    push(at, close, if b[at] == b'#' { "param" } else { "param.raw" });
    close
}

/// A single-quoted literal starting at `at`, cut around any binding inside it — `'%${name}%'` is the
/// everyday way a `${}` appears, and swallowing it into the string would hide the one thing in the
/// statement that most deserves to stand out. Returns where the scan resumes.
fn string(b: &[u8], at: usize, end: usize, push: &mut impl FnMut(usize, usize, &str)) -> usize {
    let mut i = at + 1;
    let mut piece = at;
    while i < end {
        match b[i] {
            b'\'' if b.get(i + 1) == Some(&b'\'') => i += 2,
            b'\'' => {
                i += 1;
                break;
            }
            b'#' | b'$' if b.get(i + 1) == Some(&b'{') => {
                push(piece, i, "sql.string");
                i = binding(b, i, end, push);
                piece = i;
            }
            _ => i += 1,
        }
    }
    push(piece, i.min(end), "sql.string");
    i.min(end)
}

/// Bindings found anywhere in `from..to`, for a span whose own colour is left alone.
fn bindings_in(b: &[u8], from: usize, to: usize, push: &mut impl FnMut(usize, usize, &str)) {
    let mut i = from;
    while i < to {
        if matches!(b[i], b'#' | b'$') && b.get(i + 1) == Some(&b'{') {
            i = binding(b, i, to, push);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(text, kind)` for every highlight of `sql`, kind without its `mybatis.` prefix.
    fn lex(sql: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        highlight(sql, 0, sql.len(), &mut out);
        out.into_iter()
            .map(|h| (sql[h.start..h.end].to_string(), h.kind.trim_start_matches("mybatis.").to_string()))
            .collect()
    }

    fn has(sql: &str, text: &str, kind: &str) -> bool {
        lex(sql).iter().any(|(t, k)| t == text && k == kind)
    }

    #[test]
    fn keywords_and_functions_are_told_apart() {
        let sql = "select count(*) from orders where id = 1";
        assert!(has(sql, "select", "sql.keyword"));
        assert!(has(sql, "count", "sql.function"));
        assert!(has(sql, "where", "sql.keyword"));
        assert!(has(sql, "1", "sql.number"));
    }

    #[test]
    fn a_function_name_without_a_call_is_not_a_function() {
        assert!(lex("select count from t").iter().all(|(t, _)| t != "count"));
    }

    #[test]
    fn a_table_that_collides_with_a_keyword_is_a_name() {
        let sql = "select * from order join group on 1 = 1";
        assert!(!has(sql, "order", "sql.keyword"));
        assert!(!has(sql, "group", "sql.keyword"));
        assert!(has(sql, "join", "sql.keyword"));
    }

    #[test]
    fn a_word_after_a_dot_is_a_column() {
        assert!(!has("select o.order from o", "order", "sql.keyword"));
    }

    #[test]
    fn bindings_are_found_and_the_raw_one_is_its_own_kind() {
        let sql = "where a = #{id,jdbcType=INTEGER} and b = ${col}";
        assert!(has(sql, "#{id,jdbcType=INTEGER}", "param"));
        assert!(has(sql, "${col}", "param.raw"));
    }

    #[test]
    fn a_binding_inside_a_string_is_cut_out_of_it() {
        let sql = "like '%${name}%'";
        assert!(has(sql, "'%", "sql.string"));
        assert!(has(sql, "${name}", "param.raw"));
        assert!(has(sql, "%'", "sql.string"));
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        assert!(has("where s = 'it''s'", "'it''s'", "sql.string"));
    }

    #[test]
    fn comments_swallow_what_would_otherwise_be_keywords() {
        let sql = "select 1 -- from nowhere\n/* where 1 */";
        assert!(has(sql, "-- from nowhere", "sql.comment"));
        assert!(has(sql, "/* where 1 */", "sql.comment"));
        assert_eq!(lex(sql).iter().filter(|(_, k)| k == "sql.keyword").count(), 1);
    }

    #[test]
    fn entities_are_not_read_as_words() {
        assert!(lex("a &lt;&gt; b &amp;&amp; c").is_empty());
    }

    #[test]
    fn spans_are_in_the_buffers_own_offsets() {
        let src = "<select>select 1</select>";
        let mut out = Vec::new();
        highlight(src, 8, 16, &mut out);
        assert_eq!(&src[out[0].start..out[0].end], "select");
    }

    #[test]
    fn an_unterminated_string_or_binding_stays_inside_its_run() {
        let sql = "where a = 'open #{x";
        let mut out = Vec::new();
        highlight(sql, 0, sql.len(), &mut out);
        assert!(out.iter().all(|h| h.end <= sql.len()));
    }
}
