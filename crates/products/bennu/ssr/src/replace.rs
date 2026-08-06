//! The replacement half: a template, and the edits it produces.
//!
//! ## The template is code too
//!
//! `Optional.ofNullable($a$).map(X::$m$)` is Java with the captures put back. Same holes, same
//! spelling — there is nothing extra to learn, and a template can be pasted out of the editor and
//! have its varying parts replaced with `$name$`.
//!
//! ## What it refuses to do
//!
//! A template naming a capture the pattern never binds is a template that would silently produce
//! `Optional.ofNullable()` — valid text, wrong code, and no error anywhere. So the names are
//! checked **against the query** before a single file is read: an unknown `$name$` is refused with
//! the list of what the pattern actually binds.
//!
//! ## Edits are non-overlapping and applied back to front
//!
//! `find_all` never reports a match inside another (a replacement rewrites a whole range, so an
//! inner edit would land in text that no longer exists), and the hits from `or` are already
//! de-duplicated by range. Applying from the end means no offset needs adjusting as it goes —
//! every remaining edit still indexes into bytes it has not touched.

use std::collections::BTreeSet;

use arbor_syntax::prelude::ByteRange;

use crate::engine::Hit;
use crate::query::{Ask, Query};

/// One rewrite: replace these bytes with this text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    pub range: ByteRange,
    pub text: String,
}

/// Why a replacement was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplaceError {
    pub message: String,
}

impl std::fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Check a template against the query that will feed it.
///
/// Run once, before the search — a name that cannot be filled is a mistake in the template, not
/// a property of any file, and finding out per-file would report it hundreds of times.
pub fn check(query: &Query, template: &str) -> Result<(), ReplaceError> {
    let Ask::Patterns(alternatives) = &query.ask else {
        return Err(ReplaceError {
            message: "`use of` answers a question about the project; it does not describe a \
                      shape to rewrite, so there is nothing for a replacement to act on"
                .to_string(),
        });
    };

    let wanted = names_in(template);
    if wanted.is_empty() {
        return Ok(());
    }
    for (index, alt) in alternatives.iter().enumerate() {
        let bound = names_in(&alt.pattern);
        let missing: Vec<&String> = wanted.iter().filter(|n| !bound.contains(*n)).collect();
        if let Some(name) = missing.first() {
            let mut known: Vec<&str> = bound.iter().map(String::as_str).collect();
            known.sort_unstable();
            let known = if known.is_empty() {
                "nothing".to_string()
            } else {
                known.iter().map(|n| format!("${n}$")).collect::<Vec<_>>().join(", ")
            };
            return Err(ReplaceError {
                message: format!(
                    "the replacement uses ${name}$, which the {} pattern does not bind — it \
                     binds {known}",
                    if alternatives.len() == 1 { "" } else { ordinal(index) }.trim(),
                ),
            });
        }
    }
    Ok(())
}

fn ordinal(i: usize) -> &'static str {
    match i {
        0 => "first",
        1 => "second",
        2 => "third",
        _ => "last",
    }
}

/// Every `$name$` / `$name...$` in `text`, deduplicated.
///
/// A `BTreeSet` so the "it binds …" message lists them in a stable order — an error whose wording
/// changes between runs is an error people stop reading.
fn names_in(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '$') else { break };
        let inner: String = chars[i + 1..close].iter().collect();
        let name = inner.trim().trim_end_matches("...").trim();
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            out.insert(name.to_string());
        }
        i = close + 1;
    }
    out
}

/// Fill `template` from one hit.
///
/// A name the hit did not bind — a `...` that matched nothing — becomes the empty string, which
/// is what "it matched no siblings" means: `f($args...$)` with nothing captured is `f()`.
pub fn render(template: &str, hit: &Hit) -> String {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::with_capacity(template.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '$') else {
            out.extend(&chars[i..]);
            break;
        };
        let inner: String = chars[i + 1..close].iter().collect();
        let name = inner.trim().trim_end_matches("...").trim();
        match hit.capture(name) {
            Some(capture) => out.push_str(&capture.text),
            // Not a hole left open: a `$` pair that names nothing is text the user typed, and
            // putting it back is the only reading that does not lose it.
            None if name.is_empty() => out.push_str(&format!("${inner}$")),
            None => {}
        }
        i = close + 1;
    }
    out
}

/// The edits for one file's hits.
pub fn edits_for(template: &str, hits: &[Hit]) -> Vec<Edit> {
    hits.iter()
        .map(|hit| Edit { range: hit.range, text: render(template, hit) })
        .collect()
}

/// Apply `edits` to `source`.
///
/// Back to front, so no offset has to be adjusted as it goes. Overlapping edits are impossible by
/// construction (see the module doc); one that arrives anyway is skipped rather than corrupting
/// the file, because a rewrite that lands half in and half out of another is not recoverable.
pub fn apply(source: &str, edits: &[Edit]) -> String {
    let mut ordered: Vec<&Edit> = edits.iter().collect();
    ordered.sort_by_key(|e| std::cmp::Reverse(e.range.start));

    let mut out = source.to_string();
    let mut lowest = usize::MAX;
    for edit in ordered {
        if edit.range.end > lowest || edit.range.end > out.len() {
            continue;
        }
        if !out.is_char_boundary(edit.range.start) || !out.is_char_boundary(edit.range.end) {
            continue;
        }
        out.replace_range(edit.range.start..edit.range.end, &edit.text);
        lowest = edit.range.start;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_syntax::prelude::ByteRange;

    use crate::engine::HitCapture;
    use crate::query::parse;

    fn hit(range: (usize, usize), captures: &[(&str, &str)]) -> Hit {
        Hit {
            file: "A.java".to_string(),
            range: ByteRange::new(range.0, range.1),
            line: 1,
            preview: String::new(),
            captures: captures
                .iter()
                .map(|(n, t)| HitCapture {
                    name: (*n).to_string(),
                    range: ByteRange::new(0, 0),
                    text: (*t).to_string(),
                })
                .collect(),
            enclosing: None,
            unresolved: false,
        }
    }

    #[test]
    fn a_template_puts_the_captures_back() {
        let h = hit((0, 0), &[("a", "order"), ("m", "total")]);
        assert_eq!(
            render("Optional.ofNullable($a$).map(X::$m$)", &h),
            "Optional.ofNullable(order).map(X::total)",
        );
    }

    #[test]
    fn a_run_that_matched_nothing_renders_as_nothing() {
        let h = hit((0, 0), &[]);
        assert_eq!(render("f($args...$)", &h), "f()");
    }

    /// The check that stops a silent wrong rewrite: `$b$` would have rendered as empty, giving
    /// valid Java that does the wrong thing, with no error anywhere.
    #[test]
    fn a_template_naming_something_the_pattern_does_not_bind_is_refused() {
        let query = parse("f($a$)").unwrap();
        let e = check(&query, "g($b$)").expect_err("refused");
        assert!(e.message.contains("$b$"), "{}", e.message);
        assert!(e.message.contains("$a$"), "it says what IS available: {}", e.message);
    }

    #[test]
    fn a_template_is_checked_against_every_alternative() {
        let query = parse("f($a$)\nor g($a$, $b$)").unwrap();
        assert!(check(&query, "h($a$)").is_ok());
        let e = check(&query, "h($b$)").expect_err("the first branch has no $b$");
        assert!(e.message.contains("first"), "{}", e.message);
    }

    #[test]
    fn a_use_of_query_has_nothing_to_rewrite() {
        let query = parse("use of place on com.acme.X").unwrap();
        assert!(check(&query, "anything").is_err());
    }

    #[test]
    fn a_template_with_no_holes_is_fine() {
        assert!(check(&parse("f($a$)").unwrap(), "throw new UnsupportedOperationException()").is_ok());
    }

    // ── applying ────────────────────────────────────────────────────────────────

    #[test]
    fn edits_apply_back_to_front_so_the_later_ones_still_fit() {
        let source = "aaa bbb ccc";
        let edits = vec![
            Edit { range: ByteRange::new(0, 3), text: "X".to_string() },
            Edit { range: ByteRange::new(8, 11), text: "YY".to_string() },
        ];
        assert_eq!(apply(source, &edits), "X bbb YY");
    }

    #[test]
    fn an_overlapping_edit_is_skipped_rather_than_corrupting_the_file() {
        let source = "aaaabbbb";
        let edits = vec![
            Edit { range: ByteRange::new(0, 6), text: "X".to_string() },
            Edit { range: ByteRange::new(4, 8), text: "Y".to_string() },
        ];
        // The later one wins (applied first, back to front); the one that would land inside it
        // is dropped, and the file stays consistent.
        assert_eq!(apply(source, &edits), "aaaaY");
    }

    #[test]
    fn an_edit_that_would_split_a_character_is_skipped() {
        let source = "città";
        // Byte 4 is the middle of the two-byte `à`.
        let edits = vec![Edit { range: ByteRange::new(0, 4), text: "X".to_string() }];
        assert_eq!(apply(source, &edits), "città", "unchanged rather than mangled");
    }

    #[test]
    fn edits_for_renders_one_per_hit() {
        let hits = [
            hit((0, 3), &[("a", "one")]),
            hit((10, 13), &[("a", "two")]),
        ];
        let edits = edits_for("f($a$)", &hits);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].text, "f(one)");
        assert_eq!(edits[1].text, "f(two)");
    }
}
