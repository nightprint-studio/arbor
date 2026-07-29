//! Replacement templates, and applying what they produce.
//!
//! ## The template is source text too
//!
//! `INSERT INTO NUOVA ($cols...$) VALUES ($vals...$)` — the same shape as the
//! pattern, read the other way. `$name$` writes back what that placeholder
//! captured, **byte for byte from the subject**, so a value keeps its quoting, its
//! casing and its comments without this crate ever having to reconstruct them.
//!
//! `$name.0$` addresses one element of a list capture, which is what makes
//! reordering expressible: `($vals.2$, $vals.0$)` is a swap, written as one.
//! Indices count **elements**, not separators — `a, b, c` is `0`, `1`, `2` — so
//! the person writing the template counts what they can see.
//!
//! ## Addressing by a parallel list
//!
//! A position is a poor address exactly where it matters most. If some statements
//! write their columns in one order and some in another, `$vals.0$` means a
//! different thing in each — which is the bug, not the fix.
//!
//! So `$vals[cols=keycode]$` reads as **"the element of `vals` at the index where
//! `cols` is `keycode`"**. Both captures are lists, they are matched positionally
//! by whatever produced them, and the template addresses one *through* the other.
//! A template written this way is order-independent: it normalises every statement
//! to one shape whatever shape it was written in.
//!
//! Nothing here knows that those two lists are an `INSERT`'s columns and values —
//! only that they are parallel, which is a property of the pattern, not of SQL.
//! Which list holds the name has to be **named**: a shorthand that guessed would
//! be a rewriting tool guessing, and this one writes into somebody's database.

use crate::error::SyntaxError;
use crate::pattern::{Arity, Capture, Match};
use crate::range::ByteRange;

/// A replacement: these bytes, instead of those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: ByteRange,
    pub text: String,
}

/// Render `template` for one match, reading captures out of `source`.
///
/// Names are compared exactly. Use [`render_with`] for a language where they are
/// not case-sensitive.
pub fn render(template: &str, found: &Match, source: &str) -> Result<String, SyntaxError> {
    render_with(template, found, source, false)
}

/// The same, told whether a `[list=name]` lookup should ignore case.
///
/// The caller's decision, exactly as it is for the pattern's own leaves: SQL
/// folds an unquoted name and Java does not, and this crate refuses to infer
/// which from the grammar.
pub fn render_with(
    template: &str,
    found: &Match,
    source: &str,
    case_insensitive: bool,
) -> Result<String, SyntaxError> {
    let mut out = String::with_capacity(template.len());
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        let Some(close) = (i + 1..chars.len()).find(|&j| chars[j] == '$') else {
            return Err(SyntaxError::Template(
                "a placeholder in the replacement is never closed — write $name$, or $$ for a \
                 literal dollar sign"
                    .to_string(),
            ));
        };
        let reference: String = chars[i + 1..close].iter().collect();
        out.push_str(&resolve(reference.trim(), found, source, case_insensitive)?);
        i = close + 1;
    }
    Ok(out)
}

/// How a template names one thing.
enum Reference<'a> {
    /// `$name$` — the whole capture.
    Whole(&'a str),
    /// `$name.2$` — one element, by position.
    At(&'a str, usize),
    /// `$vals[cols=keycode]$` — one element, by the position a *parallel* list
    /// holds a name at.
    Where { name: &'a str, through: &'a str, key: &'a str },
}

fn parse_reference(reference: &str) -> Result<Reference<'_>, SyntaxError> {
    if let Some(open) = reference.find('[') {
        let Some(close) = reference.strip_suffix(']') else {
            return Err(SyntaxError::Template(format!(
                "{reference} is missing its closing bracket — write $values[columns=keycode]$"
            )));
        };
        let inside = &close[open + 1..];
        let Some((through, key)) = inside.split_once('=') else {
            return Err(SyntaxError::Template(format!(
                "{reference}: say which list holds {inside} — write \
                 $values[columns={inside}]$, meaning \"the element of values where columns is \
                 {inside}\". It is not guessed: a rewrite that picked the wrong list would be a \
                 rewrite into the wrong column."
            )));
        };
        return Ok(Reference::Where {
            name: reference[..open].trim(),
            through: through.trim(),
            key: key.trim(),
        });
    }
    match reference.split_once('.') {
        Some((name, index)) => {
            let parsed = index.trim().parse::<usize>().map_err(|_| {
                SyntaxError::Template(format!(
                    "{reference} is not a usable reference — write $name$, $name.0$ or \
                     $name[other=value]$"
                ))
            })?;
            Ok(Reference::At(name.trim(), parsed))
        }
        None => Ok(Reference::Whole(reference)),
    }
}

fn resolve(
    reference: &str,
    found: &Match,
    source: &str,
    case_insensitive: bool,
) -> Result<String, SyntaxError> {
    match parse_reference(reference)? {
        Reference::Whole(name) => {
            let capture = capture_of(found, name)?;
            Ok(capture.range.slice(source).unwrap_or("").to_string())
        }
        Reference::At(name, index) => {
            let capture = capture_of(found, name)?;
            if capture.arity == Arity::One && index > 0 {
                return Err(SyntaxError::Template(format!(
                    "{name} is a single placeholder, so it has one element — write $name...$ in \
                     the pattern if it should match a list"
                )));
            }
            let elements = elements_of(capture, source);
            let element = elements.get(index).ok_or_else(|| {
                SyntaxError::Template(format!(
                    "{name} matched {} element{} — there is no {index}",
                    elements.len(),
                    if elements.len() == 1 { "" } else { "s" }
                ))
            })?;
            Ok(element.slice(source).unwrap_or("").to_string())
        }
        Reference::Where { name, through, key } => {
            let target = capture_of(found, name)?;
            let index_list = capture_of(found, through)?;
            let names = elements_of(index_list, source);
            let values = elements_of(target, source);

            // Parallel means parallel. Two lists of different lengths have no
            // position in common to look up, and pairing them anyway would write a
            // value into a column it does not belong to — the exact failure this
            // whole form exists to prevent.
            if names.len() != values.len() {
                return Err(SyntaxError::Template(format!(
                    "{through} has {} element{} and {name} has {} — they cannot be read as one \
                     list against the other",
                    names.len(),
                    if names.len() == 1 { "" } else { "s" },
                    values.len()
                )));
            }

            let at = names.iter().position(|range| {
                let text = range.slice(source).unwrap_or("").trim();
                if case_insensitive { text.eq_ignore_ascii_case(key) } else { text == key }
            });
            let at = at.ok_or_else(|| {
                SyntaxError::Template(format!(
                    "{through} does not hold {key} here — it holds {}",
                    names
                        .iter()
                        .map(|r| r.slice(source).unwrap_or("").trim())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            Ok(values[at].slice(source).unwrap_or("").to_string())
        }
    }
}

fn capture_of<'a>(found: &'a Match, name: &str) -> Result<&'a Capture, SyntaxError> {
    found.capture(name).ok_or_else(|| {
        SyntaxError::Template(format!("the pattern has no placeholder called {name}"))
    })
}

/// The elements of a list capture — not the separators.
///
/// The anonymous nodes between them are what the grammar puts there, and nobody
/// counting a column list counts the commas.
fn elements_of<'a>(capture: &'a Capture, source: &str) -> Vec<&'a ByteRange> {
    capture
        .parts
        .iter()
        .filter(|part| {
            part.slice(source).map(|t| t.trim()).is_some_and(|t| {
                !t.is_empty() && t.chars().any(|c| c.is_alphanumeric() || c == '\'' || c == '"')
            })
        })
        .collect()
}

/// Apply edits to `source`, right to left so earlier ranges keep their offsets.
///
/// Overlapping edits are a bug in the caller and are reported as one rather than
/// silently resolved: whichever way a resolution went, half the intent would be
/// lost and nothing would say which half.
pub fn apply(source: &str, edits: &[TextEdit]) -> Result<String, SyntaxError> {
    let mut ordered: Vec<&TextEdit> = edits.iter().collect();
    ordered.sort_by_key(|e| e.range.start);
    for pair in ordered.windows(2) {
        if pair[0].range.overlaps(&pair[1].range) {
            return Err(SyntaxError::Template(format!(
                "two replacements cover the same bytes ({}..{} and {}..{})",
                pair[0].range.start, pair[0].range.end, pair[1].range.start, pair[1].range.end
            )));
        }
    }

    let mut out = source.to_string();
    for edit in ordered.iter().rev() {
        if out.get(edit.range.start..edit.range.end).is_none() {
            return Err(SyntaxError::Template(format!(
                "a replacement names bytes {}..{}, which are not a boundary of this text",
                edit.range.start, edit.range.end
            )));
        }
        out.replace_range(edit.range.start..edit.range.end, &edit.text);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Pattern;
    use tree_sitter::Language;

    fn java() -> Language {
        tree_sitter_java::LANGUAGE.into()
    }

    fn in_a_method(pattern: &str) -> Pattern {
        Pattern::compile_in(&java(), pattern, "class C { void m() { ", " } }").expect("compiles")
    }

    #[test]
    fn a_template_writes_back_the_captured_bytes() {
        let pattern = in_a_method("registra($what$);");
        let source = "class A { void go() { registra(codice); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        let text = render("annota($what$, \"vecchio\");", &found[0], source).expect("renders");
        assert_eq!(text, "annota(codice, \"vecchio\");");
    }

    #[test]
    fn a_list_can_be_reordered_by_index() {
        // The transformation the whole feature exists for: same values, new order.
        let pattern = in_a_method("registra($v...$);");
        let source = "class A { void go() { registra(uno, due, tre); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        let text = render("registra($v.2$, $v.0$, $v.1$);", &found[0], source).expect("renders");
        assert_eq!(text, "registra(tre, uno, due);");
    }

    #[test]
    fn indexing_past_the_end_says_how_many_there_were() {
        let pattern = in_a_method("registra($v...$);");
        let source = "class A { void go() { registra(uno, due); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        let err = render("registra($v.5$);", &found[0], source).expect_err("refused");
        assert!(err.to_string().contains("matched 2 elements"), "{err}");
    }

    // ── Addressing one list through a parallel one ────────────────────────────
    //
    // The fixture mirrors the shape this exists for — `INSERT INTO t (cols) VALUES
    // (vals)`: two lists in **separately delimited** groups. Two list placeholders
    // adjacent inside ONE flat list have no unambiguous split, and the matcher's
    // greedy-then-backtrack rule would take the whole thing for the first, which
    // is pinned by `two_list_placeholders_backtrack_into_place`.

    /// A call shaped like an INSERT: names in one group, values in another.
    fn parallel() -> Pattern {
        in_a_method("registra(nomi($n...$), valori($v...$));")
    }

    fn call(names: &str, values: &str) -> String {
        format!("class A {{ void go() {{ registra(nomi({names}), valori({values})); }} }}")
    }

    #[test]
    fn one_list_can_be_addressed_through_a_parallel_one() {
        // The reason this form exists: a position is a poor address exactly where
        // it matters, because the two calls below write their arguments in
        // different orders and `$v.0$` means a different thing in each.
        let pattern = parallel();
        for (names, values) in
            [("chiave, lingua", "'K', 'IT'"), ("lingua, chiave", "'IT', 'K'")]
        {
            let source = call(names, values);
            let found = pattern.find_all(&java(), &source).expect("searches");
            assert_eq!(found.len(), 1, "{source}");
            // The same reference, both orders, the same answer.
            assert_eq!(render("$v[n=chiave]$", &found[0], &source).expect("renders"), "'K'");
        }
    }

    #[test]
    fn a_lookup_normalises_the_statement_whatever_order_it_was_written_in() {
        // What it is for, end to end: one template, two orders in, one order out.
        let pattern = parallel();
        let template = "registra(nomi(chiave, lingua), valori($v[n=chiave]$, $v[n=lingua]$));";
        for (names, values) in
            [("chiave, lingua", "'K', 'IT'"), ("lingua, chiave", "'IT', 'K'")]
        {
            let source = call(names, values);
            let found = pattern.find_all(&java(), &source).expect("searches");
            assert_eq!(
                render(template, &found[0], &source).expect("renders"),
                "registra(nomi(chiave, lingua), valori('K', 'IT'));",
                "{source}"
            );
        }
    }

    #[test]
    fn lists_of_different_lengths_are_refused_rather_than_paired_anyway() {
        // Pairing them would write a value into a column it does not belong to,
        // which is the exact failure this form exists to prevent.
        let pattern = parallel();
        let source = call("chiave, lingua", "'K'");
        let found = pattern.find_all(&java(), &source).expect("searches");
        let err = render("$v[n=chiave]$", &found[0], &source).expect_err("refused");
        assert!(err.to_string().contains("cannot be read as one list"), "{err}");
    }

    #[test]
    fn a_name_the_index_list_does_not_hold_says_what_it_does_hold() {
        let pattern = parallel();
        let source = call("chiave, lingua", "'K', 'IT'");
        let found = pattern.find_all(&java(), &source).expect("searches");
        let err = render("$v[n=valore]$", &found[0], &source).expect_err("refused");
        assert!(err.to_string().contains("it holds chiave, lingua"), "{err}");
    }

    #[test]
    fn which_list_holds_the_name_is_never_guessed() {
        // A shorthand that picked a list would be a rewriting tool guessing, and
        // the message has to teach the long form rather than just refuse.
        let pattern = parallel();
        let source = call("chiave, lingua", "'K', 'IT'");
        let found = pattern.find_all(&java(), &source).expect("searches");
        let err = render("$v[chiave]$", &found[0], &source).expect_err("refused");
        assert!(err.to_string().contains("$values[columns=chiave]$"), "{err}");
    }

    #[test]
    fn a_lookup_can_ignore_case_when_the_caller_says_so() {
        let pattern = parallel();
        let source = call("CHIAVE, lingua", "'K', 'IT'");
        let found = pattern.find_all(&java(), &source).expect("searches");
        assert!(render("$v[n=chiave]$", &found[0], &source).is_err(), "exact by default");
        assert_eq!(
            render_with("$v[n=chiave]$", &found[0], &source, true).expect("renders"),
            "'K'"
        );
    }

    #[test]
    fn naming_a_placeholder_the_pattern_never_had_is_refused() {
        let pattern = in_a_method("registra($what$);");
        let source = "class A { void go() { registra(x); } }";
        let found = pattern.find_all(&java(), source).expect("searches");
        let err = render("registra($altro$);", &found[0], source).expect_err("refused");
        assert!(err.to_string().contains("no placeholder called altro"), "{err}");
    }

    #[test]
    fn edits_apply_right_to_left_so_earlier_offsets_survive() {
        let source = "uno due tre";
        let text = apply(
            source,
            &[
                TextEdit { range: ByteRange::new(0, 3), text: "UNO".into() },
                TextEdit { range: ByteRange::new(8, 11), text: "TRE!!!".into() },
            ],
        )
        .expect("applies");
        assert_eq!(text, "UNO due TRE!!!");
    }

    #[test]
    fn overlapping_edits_are_refused_rather_than_resolved() {
        let err = apply(
            "uno due tre",
            &[
                TextEdit { range: ByteRange::new(0, 5), text: "a".into() },
                TextEdit { range: ByteRange::new(4, 8), text: "b".into() },
            ],
        )
        .expect_err("refused");
        assert!(err.to_string().contains("same bytes"), "{err}");
    }

    #[test]
    fn everything_outside_the_edits_survives_byte_for_byte() {
        // The invariant a rewrite of somebody's repository rests on.
        let source = "class A {\r\n  void go() {\r\n    // perché\r\n    registra(x);\r\n  }\r\n}";
        let pattern = in_a_method("registra($w$);");
        let found = pattern.find_all(&java(), source).expect("searches");
        let edits: Vec<TextEdit> = found
            .iter()
            .map(|m| TextEdit { range: m.range, text: render("annota($w$);", m, source).unwrap() })
            .collect();
        assert_eq!(edits.len(), 1);
        let out = apply(source, &edits).expect("applies");
        // CRLF, the accented comment and every space are exactly as they were:
        // only the matched bytes moved.
        assert_eq!(out, "class A {\r\n  void go() {\r\n    // perché\r\n    annota(x);\r\n  }\r\n}");
    }
}
