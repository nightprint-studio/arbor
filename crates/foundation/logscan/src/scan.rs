//! The scanner: walk a line, ask the rules at every token start, collect the parts.
//!
//! ## Why "token start" and not "every position"
//!
//! A rule asked at every byte would match `ERROR` inside `myERRORflag` and `http://` inside
//! `xhttp://y`. So rules are only offered positions that begin a token — the start of the
//! line, or anything just after a [boundary](is_boundary) character. It also makes the walk
//! cheap: text that matches nothing is skipped a whole token at a time.
//!
//! Boundaries are the characters that cannot appear *inside* the things the rules
//! recognise. Notably `.`, `/`, `\`, `:`, `-`, `_`, `@`, `$`, `#`, `~` and `+` are **not**
//! boundaries: they are inside qualified names, paths, URLs, timestamps and inner-class
//! names, and treating them as separators would cut every interesting construct in half.

use crate::rule::{Part, RuleSet};

/// Characters that separate tokens. Everything else is "inside" something.
pub fn is_boundary(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '[' | ']' | '(' | ')' | '{' | '}' | '<' | '>' | ',' | ';' | '"' | '\'' | '=' | '|' | '`' | '!' | '?' | '*' | '&' | '%' | '^'
        )
}

/// The end of the token starting at `at`: the next boundary, or the end of the line.
/// Always a char boundary, and always `> at` when `text[at]` is not itself a boundary.
pub fn token_end(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, c)| is_boundary(*c))
        .map(|(i, _)| at + i)
        .unwrap_or(text.len())
}

/// Whether `at` begins a token — the start of the line, or just after a boundary.
fn is_token_start(text: &str, at: usize) -> bool {
    at == 0 || text[..at].chars().next_back().map(is_boundary).unwrap_or(true)
}

/// Run `rules` over `text`, returning the annotated parts in order, non-overlapping.
///
/// The first rule to hit at a position wins and scanning resumes where it stopped — so a
/// rule that consumed a whole construct protects its own insides from the rules below it.
pub fn scan(rules: &RuleSet, text: &str) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    let mut at = 0usize;
    while at < text.len() {
        if is_token_start(text, at) {
            let mut hit = None;
            for rule in rules.rules() {
                if let Some(h) = rule.match_at(text, at) {
                    // A rule that consumes nothing is ignored: trusting it would be
                    // trusting it to terminate the loop.
                    if h.end > at && h.end <= text.len() && text.is_char_boundary(h.end) {
                        hit = Some(h);
                        break;
                    }
                }
            }
            if let Some(h) = hit {
                for p in h.parts {
                    // Defensive: a rule is not trusted to keep its parts inside the line,
                    // in order, or off each other.
                    let ok = p.end > p.start
                        && p.end <= text.len()
                        && text.is_char_boundary(p.start)
                        && text.is_char_boundary(p.end)
                        && out.last().map(|l| p.start >= l.end).unwrap_or(true);
                    if ok {
                        out.push(p);
                    }
                }
                at = h.end;
                continue;
            }
        }
        // Nothing here: step over one boundary character, or over the whole token.
        let c = match text[at..].chars().next() {
            Some(c) => c,
            None => break,
        };
        at = if is_boundary(c) { at + c.len_utf8() } else { token_end(text, at) };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Token;
    use crate::rule::{FnRule, Hit};

    /// A rule matching the literal `foo`, for testing the walk itself.
    fn foo_at(text: &str, at: usize) -> Option<Hit> {
        let end = token_end(text, at);
        (&text[at..end] == "foo").then(|| Hit::one(at, end, Token::Package))
    }

    fn foo_rule() -> FnRule<fn(&str, usize) -> Option<Hit>> {
        FnRule::new("foo", foo_at as fn(&str, usize) -> Option<Hit>)
    }

    #[test]
    fn a_rule_only_sees_token_starts() {
        let rules = RuleSet::empty().with(foo_rule());
        // `barfoo` contains `foo`, but not at a token start.
        assert!(scan(&rules, "barfoo").is_empty());
        assert_eq!(scan(&rules, "bar foo").len(), 1);
        assert_eq!(scan(&rules, "(foo)").len(), 1);
    }

    #[test]
    fn the_first_rule_to_hit_wins() {
        let rules = RuleSet::empty()
            .with(FnRule::new("all", |_: &str, at: usize| Some(Hit::one(at, at + 3, Token::Url))))
            .with(foo_rule());
        let parts = scan(&rules, "foo");
        assert_eq!(parts[0].token, Token::Url);
    }

    #[test]
    fn a_rule_that_consumes_nothing_cannot_hang_the_scan() {
        let rules =
            RuleSet::empty().with(FnRule::new("stuck", |_: &str, at: usize| Some(Hit::spanning(at))));
        assert!(scan(&rules, "one two three").is_empty());
    }

    #[test]
    fn non_ascii_text_is_walked_by_characters() {
        let rules = RuleSet::empty().with(foo_rule());
        let parts = scan(&rules, "città foo però");
        assert_eq!(parts.len(), 1);
        assert_eq!(&"città foo però"[parts[0].start..parts[0].end], "foo");
    }

    #[test]
    fn out_of_order_parts_from_a_bad_rule_are_dropped_not_rendered() {
        let rules = RuleSet::empty().with(FnRule::new("messy", |_: &str, at: usize| {
            Some(Hit::spanning(at + 3).part(at, at + 3, Token::Url, None).part(at, at + 2, Token::Path, None))
        }));
        let parts = scan(&rules, "abc");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].token, Token::Url);
    }
}
