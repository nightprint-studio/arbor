//! SpEL — the Spring Expression Language, `#{ … }`, tokenized rather than evaluated.
//!
//! An editor does not need to *run* `#{@userService.findAll().?[active]}`. It needs to
//! know that `@userService` is a bean reference (so Ctrl+B goes to it), that `'admin'` is
//! a string and not an identifier (so it is coloured as one), and that the brace closes.
//! That is what this is: a scanner producing spans, plus the short list of things that
//! are broken as a matter of fact rather than of taste.
//!
//! ## What is deliberately NOT here
//!
//! No AST, no type checking, no operator-precedence table. SpEL has method calls,
//! projections (`.![…]`), selections (`.?[…]`), safe navigation, ternaries, inline lists
//! and maps, `T(java.lang.Math)` type references and bean references — and getting any of
//! that *wrong* in a squiggle is worse than not having it. The issue list carries three
//! things only: an unclosed `#{`, an unterminated string literal, and unbalanced
//! brackets. Everything else is left to Spring at startup, which is the only thing that
//! knows.
//!
//! ## Inline lists and the closing brace
//!
//! `#{{1,2,3}}` is a valid expression whose body is an inline list, so the closing brace
//! of the expression is *not* the first `}` — and neither is it the last, since a string
//! literal can contain any brace at all. The body scan tracks brace depth and string
//! state for exactly this reason.

use crate::placeholder;

/// What a token is, for colouring and for navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// `@beanName` — a bean reference (navigable).
    BeanRef,
    /// `#variable` / `#root` / `#this` — an evaluation-context variable.
    Variable,
    /// The `T` of a `T(java.lang.Math)` type reference.
    TypeRef,
    /// A SpEL keyword (`new`, `instanceof`, `true`, `null`, the word operators `and`/`or`/…).
    Keyword,
    /// Any other identifier — a property, a method name.
    Ident,
    /// A `'…'` string literal, quotes included.
    String,
    /// A numeric literal.
    Number,
    /// An operator (`+`, `==`, `?:`, `!`, …).
    Operator,
    /// Structural punctuation (`(`, `)`, `[`, `]`, `{`, `}`, `,`, `.`).
    Punct,
    /// A `${…}` property placeholder nested inside the expression — legal, and resolved
    /// by a different mechanism, so it is handed back whole.
    Placeholder,
}

/// One lexical span inside a SpEL body. Offsets are byte offsets into the ORIGINAL
/// scanned text, not into the body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpelToken {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

/// A named reference inside an expression — a bean (`@foo`) or a context variable
/// (`#foo`). The span covers the sigil *and* the name, so highlighting one highlights
/// the whole thing; [`Self::name`] is just the name, which is what a lookup wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpelRef {
    pub start: usize,
    pub end: usize,
    pub name: String,
    /// Span of the name alone (without the `@` / `#`) — the go-to target.
    pub name_start: usize,
    pub name_end: usize,
}

/// A factual problem in an expression. See the module docs for the (short) list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpelIssue {
    pub start: usize,
    pub end: usize,
    pub message: String,
}

/// One `#{ … }` occurrence with everything found inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpelExpr {
    /// Start of the `#` of `#{`.
    pub start: usize,
    /// End of the closing `}` (exclusive); end of text when unterminated.
    pub end: usize,
    /// Body span (between the braces).
    pub body_start: usize,
    pub body_end: usize,
    pub terminated: bool,
    pub tokens: Vec<SpelToken>,
    /// Every `@bean` reference, in source order.
    pub bean_refs: Vec<SpelRef>,
    /// Every `#variable` reference, in source order.
    pub variables: Vec<SpelRef>,
    pub issues: Vec<SpelIssue>,
}

/// Every `#{ … }` expression in `text`, in source order. Never fails.
pub fn expressions(text: &str) -> Vec<SpelExpr> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] != b'#' || b[i + 1] != b'{' {
            i += 1;
            continue;
        }
        let body_start = i + 2;
        let (body_end, terminated) = match_body(b, body_start);
        let mut expr = SpelExpr {
            start: i,
            end: if terminated { body_end + 1 } else { body_end },
            body_start,
            body_end,
            terminated,
            tokens: Vec::new(),
            bean_refs: Vec::new(),
            variables: Vec::new(),
            issues: Vec::new(),
        };
        if !terminated {
            expr.issues.push(SpelIssue {
                start: i,
                end: body_end,
                message: "Unclosed SpEL expression — expected `}`".to_string(),
            });
        }
        tokenize(text, body_start, body_end, &mut expr);
        if expr.tokens.is_empty() && terminated {
            expr.issues.push(SpelIssue {
                start: i,
                end: expr.end,
                message: "Empty SpEL expression".to_string(),
            });
        }
        i = expr.end.max(i + 2);
        out.push(expr);
    }
    out
}

/// The expression whose span covers `offset`, if any.
pub fn expression_at(text: &str, offset: usize) -> Option<SpelExpr> {
    expressions(text).into_iter().find(|e| offset >= e.start && offset <= e.end)
}

/// The bean reference under `offset` across every expression in `text` — what a go-to /
/// hover on `@userService` resolves against.
pub fn bean_ref_at(text: &str, offset: usize) -> Option<SpelRef> {
    expressions(text)
        .into_iter()
        .flat_map(|e| e.bean_refs)
        .find(|r| offset >= r.start && offset <= r.end)
}

/// Every issue across every expression in `text`.
pub fn issues(text: &str) -> Vec<SpelIssue> {
    expressions(text).into_iter().flat_map(|e| e.issues).collect()
}

/// Find the end of a SpEL body opened at `from`, tracking nested `{}` (inline lists and
/// maps) and skipping string literals, so neither a brace in a string nor an inline list
/// closes the expression early.
fn match_body(b: &[u8], from: usize) -> (usize, bool) {
    let mut depth = 0usize;
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'\'' | b'"' => {
                i = skip_string(b, i).0;
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                if depth == 0 {
                    return (i, true);
                }
                depth -= 1;
            }
            _ => {}
        }
        i += 1;
    }
    (b.len(), false)
}

/// Skip a quoted literal starting at `from` (on the quote). SpEL escapes a quote by
/// doubling it (`'it''s'`), which the doubled-quote branch handles. Returns the offset
/// just past the closing quote and whether it was found.
fn skip_string(b: &[u8], from: usize) -> (usize, bool) {
    let quote = b[from];
    let mut i = from + 1;
    while i < b.len() {
        if b[i] == quote {
            if i + 1 < b.len() && b[i + 1] == quote {
                i += 2; // doubled quote = an escaped one, keep going
                continue;
            }
            return (i + 1, true);
        }
        i += 1;
    }
    (b.len(), false)
}

/// The SpEL keywords worth colouring apart from a plain property name.
const KEYWORDS: &[&str] = &[
    "new", "instanceof", "matches", "true", "false", "null", "and", "or", "not", "div", "mod",
    "between", "empty", "gt", "ge", "lt", "le", "eq", "ne",
];

/// Tokenize `text[body_start..body_end]`, appending tokens / refs / issues to `expr`.
fn tokenize(text: &str, body_start: usize, body_end: usize, expr: &mut SpelExpr) {
    let b = text.as_bytes();
    // Bracket balance: `(`/`[` pushed with their offset so an unclosed one can point at
    // the opener rather than at the end of the expression.
    let mut open: Vec<(u8, usize)> = Vec::new();
    let mut i = body_start;
    while i < body_end {
        let c = b[i];
        // Whitespace.
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // A nested `${…}` placeholder — handed back whole, resolved elsewhere.
        if c == b'$' && i + 1 < body_end && b[i + 1] == b'{' {
            let rest = &text[i..body_end];
            if let Some(p) = placeholder::placeholders(rest).into_iter().next() {
                expr.tokens.push(SpelToken {
                    start: i + p.start,
                    end: (i + p.end).min(body_end),
                    kind: TokenKind::Placeholder,
                });
                i = (i + p.end).min(body_end).max(i + 2);
                continue;
            }
        }
        // String literal.
        if c == b'\'' || c == b'"' {
            let (end, closed) = skip_string(b, i);
            let end = end.min(body_end);
            expr.tokens.push(SpelToken { start: i, end, kind: TokenKind::String });
            if !closed {
                expr.issues.push(SpelIssue {
                    start: i,
                    end,
                    message: "Unterminated string literal in SpEL expression".to_string(),
                });
            }
            i = end.max(i + 1);
            continue;
        }
        // Bean reference / context variable: sigil + identifier.
        if (c == b'@' || c == b'#') && i + 1 < body_end && is_ident_start(b[i + 1]) {
            let name_start = i + 1;
            let mut j = name_start;
            while j < body_end && is_ident_part(b[j]) {
                j += 1;
            }
            let r = SpelRef {
                start: i,
                end: j,
                name: text[name_start..j].to_string(),
                name_start,
                name_end: j,
            };
            expr.tokens.push(SpelToken {
                start: i,
                end: j,
                kind: if c == b'@' { TokenKind::BeanRef } else { TokenKind::Variable },
            });
            if c == b'@' {
                expr.bean_refs.push(r);
            } else {
                expr.variables.push(r);
            }
            i = j;
            continue;
        }
        // Number.
        if c.is_ascii_digit() {
            let mut j = i;
            while j < body_end && (b[j].is_ascii_alphanumeric() || b[j] == b'.') {
                j += 1;
            }
            expr.tokens.push(SpelToken { start: i, end: j, kind: TokenKind::Number });
            i = j;
            continue;
        }
        // Identifier / keyword / the `T` of a type reference.
        if is_ident_start(c) {
            let mut j = i;
            while j < body_end && is_ident_part(b[j]) {
                j += 1;
            }
            let word = &text[i..j];
            // `T(` — a type reference. Whitespace between is not legal SpEL, so the
            // check is exact.
            let kind = if word == "T" && j < body_end && b[j] == b'(' {
                TokenKind::TypeRef
            } else if KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else {
                TokenKind::Ident
            };
            expr.tokens.push(SpelToken { start: i, end: j, kind });
            i = j;
            continue;
        }
        // Brackets, tracked for balance.
        if matches!(c, b'(' | b'[' | b'{') {
            open.push((c, i));
            expr.tokens.push(SpelToken { start: i, end: i + 1, kind: TokenKind::Punct });
            i += 1;
            continue;
        }
        if matches!(c, b')' | b']' | b'}') {
            let want = match c {
                b')' => b'(',
                b']' => b'[',
                _ => b'{',
            };
            match open.last() {
                Some((o, _)) if *o == want => {
                    open.pop();
                }
                _ => expr.issues.push(SpelIssue {
                    start: i,
                    end: i + 1,
                    message: format!("Unmatched `{}` in SpEL expression", c as char),
                }),
            }
            expr.tokens.push(SpelToken { start: i, end: i + 1, kind: TokenKind::Punct });
            i += 1;
            continue;
        }
        if matches!(c, b',' | b'.' | b';') {
            expr.tokens.push(SpelToken { start: i, end: i + 1, kind: TokenKind::Punct });
            i += 1;
            continue;
        }
        // Everything else is an operator character; run them together so `==` / `?:` /
        // `.?[` read as one token rather than as a stutter of single characters.
        let mut j = i;
        while j < body_end && is_operator(b[j]) {
            j += 1;
        }
        if j == i {
            j += 1; // an unclassified byte (e.g. a stray backslash) — never loop forever
        }
        expr.tokens.push(SpelToken { start: i, end: j, kind: TokenKind::Operator });
        i = j;
    }
    // Openers that never closed.
    for (c, at) in open {
        expr.issues.push(SpelIssue {
            start: at,
            end: at + 1,
            message: format!("Unclosed `{}` in SpEL expression", c as char),
        });
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80
}

fn is_ident_part(c: u8) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn is_operator(c: u8) -> bool {
    matches!(c, b'+' | b'-' | b'*' | b'/' | b'%' | b'^' | b'=' | b'<' | b'>' | b'!' | b'?' | b':' | b'&' | b'|' | b'~')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(TokenKind, &str)> {
        expressions(text)[0]
            .tokens
            .iter()
            .map(|t| (t.kind, &text[t.start..t.end]))
            .collect()
    }

    #[test]
    fn bean_reference_is_found_with_its_name_span() {
        let text = "#{@userService.findAll()}";
        let e = &expressions(text)[0];
        assert_eq!(e.bean_refs.len(), 1);
        let r = &e.bean_refs[0];
        assert_eq!(r.name, "userService");
        assert_eq!(&text[r.start..r.end], "@userService");
        assert_eq!(&text[r.name_start..r.name_end], "userService");
        assert!(e.issues.is_empty());
    }

    #[test]
    fn variables_and_beans_are_told_apart() {
        let e = &expressions("#{#root.name == @cfg.owner}")[0];
        assert_eq!(e.variables.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["root"]);
        assert_eq!(e.bean_refs.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["cfg"]);
    }

    #[test]
    fn strings_swallow_braces_and_do_not_end_the_expression() {
        let text = "#{'}' + name}";
        let e = &expressions(text)[0];
        assert!(e.terminated);
        assert_eq!(&text[e.start..e.end], "#{'}' + name}");
        assert!(e.issues.is_empty(), "a brace inside a literal is not a brace");
    }

    #[test]
    fn doubled_quote_is_an_escape() {
        let text = "#{'it''s fine'}";
        let e = &expressions(text)[0];
        assert!(e.issues.is_empty());
        assert_eq!(kinds(text)[0], (TokenKind::String, "'it''s fine'"));
    }

    #[test]
    fn inline_list_does_not_close_the_expression_early() {
        let text = "#{{1,2,3}}";
        let e = &expressions(text)[0];
        assert!(e.terminated);
        assert_eq!(&text[e.start..e.end], "#{{1,2,3}}");
        assert!(e.issues.is_empty());
    }

    #[test]
    fn type_reference_t_is_distinguished_from_a_property_named_t() {
        assert!(kinds("#{T(java.lang.Math).random()}").contains(&(TokenKind::TypeRef, "T")));
        assert!(kinds("#{T + 1}").contains(&(TokenKind::Ident, "T")));
    }

    #[test]
    fn keywords_are_tagged() {
        let ks = kinds("#{name matches '[a-z]+' and active == true}");
        assert!(ks.contains(&(TokenKind::Keyword, "matches")));
        assert!(ks.contains(&(TokenKind::Keyword, "and")));
        assert!(ks.contains(&(TokenKind::Keyword, "true")));
    }

    #[test]
    fn nested_placeholder_is_handed_back_whole() {
        let text = "#{'${app.name}'.length()}";
        // Inside a string literal it stays part of the string — the literal wins.
        assert_eq!(kinds(text)[0].0, TokenKind::String);

        let bare = "#{${app.timeout} * 2}";
        let toks = kinds(bare);
        assert_eq!(toks[0], (TokenKind::Placeholder, "${app.timeout}"));
    }

    #[test]
    fn unclosed_expression_is_reported_once() {
        let e = &expressions("#{@svc.call(")[0];
        assert!(!e.terminated);
        assert!(e.issues.iter().any(|i| i.message.contains("Unclosed SpEL")));
        assert!(e.issues.iter().any(|i| i.message.contains("Unclosed `(`")));
    }

    #[test]
    fn unmatched_closing_bracket_points_at_itself() {
        let text = "#{foo)}";
        let e = &expressions(text)[0];
        let issue = e.issues.iter().find(|i| i.message.contains("Unmatched")).expect("reported");
        assert_eq!(&text[issue.start..issue.end], ")");
    }

    #[test]
    fn unterminated_string_is_reported() {
        let e = &expressions("#{'oops}")[0];
        assert!(e.issues.iter().any(|i| i.message.contains("Unterminated string")));
    }

    #[test]
    fn empty_expression_is_reported_but_a_full_one_is_not() {
        assert!(issues("#{}").iter().any(|i| i.message == "Empty SpEL expression"));
        assert!(issues("#{1}").is_empty());
    }

    #[test]
    fn a_valid_expression_reports_nothing() {
        // The whole point: ordinary SpEL must be silent. Anything here that squiggles is
        // a false positive in a user's editor.
        for src in [
            "#{@userService.findAll().?[active].![name]}",
            "#{systemProperties['user.region']}",
            "#{ T(java.lang.Math).max(1, 2) }",
            "#{condition ? 'yes' : 'no'}",
            "#{ {name: 'a', n: 1} }",
            "#{@cfg.timeout ?: 30}",
            "#{'a' + \"b\"}",
        ] {
            assert!(issues(src).is_empty(), "false positive on `{src}`: {:?}", issues(src));
        }
    }

    #[test]
    fn several_expressions_in_one_string() {
        let text = "#{a} and #{@b}";
        let es = expressions(text);
        assert_eq!(es.len(), 2);
        assert_eq!(es[1].bean_refs[0].name, "b");
    }

    #[test]
    fn bean_ref_at_finds_the_reference_under_the_caret() {
        let text = "#{@orderService.total()}";
        let at = text.find("orderService").unwrap() + 2;
        assert_eq!(bean_ref_at(text, at).unwrap().name, "orderService");
        assert!(bean_ref_at(text, text.len() - 1).is_none());
    }

    #[test]
    fn text_without_expressions_is_empty() {
        assert!(expressions("plain, with a # and a { but never together").is_empty());
    }
}
