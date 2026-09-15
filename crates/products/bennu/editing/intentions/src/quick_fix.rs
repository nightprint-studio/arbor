//! Quick-fixes — the repair attached to a diagnostic.
//!
//! Finding the problem is the easy half. What makes an IDE feel like one is that pressing Alt+Enter
//! on the red squiggle *fixes it*: the unused import goes away, the fall-through gets its `break`,
//! the reference comparison becomes an `equals`. Bennu had seventy-one kinds of diagnostic and one
//! fix (add a missing import), so every other one was a sentence telling you to go and do something.
//!
//! ## Keyed by code and span, never by message
//!
//! A fix takes the diagnostic's `code` and its byte span and reads the *source* — it never reads the
//! message. Parsing our own prose back into data would make every message a wire format: rewording
//! "Unused import `Foo`" to something clearer would silently break the fix, and nothing would say
//! so. The `code` is the contract; the message is for the person.
//!
//! ## What is here and what is not
//!
//! Only fixes decidable from the text at the span. A fix that needs to know types — add the missing
//! `throws`, fill in an enum switch, insert a cast — needs the resolver and is built where the
//! resolver is, in the backend, from the same analysis that produced the diagnostic. The split is
//! the same one the checks themselves make.

use crate::Edit;

/// One repair for a diagnostic: a stable id, a label, and the edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub id: String,
    pub label: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

fn fix(id: &str, label: &str, e: Edit) -> Fix {
    Fix { id: id.into(), label: label.into(), start: e.start, end: e.end, replacement: e.replacement }
}

/// Every text-only fix for a diagnostic of `code` spanning `[start, end)` in `source`.
///
/// Empty when the code has no text-only fix, or when the span does not hold what the fix expects —
/// a diagnostic can outlive the text it was computed against by a keystroke, and a fix applied to
/// text that has moved on is a corruption, not a repair.
pub fn fixes_for(code: &str, source: &str, start: usize, end: usize) -> Vec<Fix> {
    if start > end || end > source.len() || !source.is_char_boundary(start) || !source.is_char_boundary(end) {
        return Vec::new();
    }
    match code {
        // Three ways of saying "this import does nothing", one repair.
        "unused-import" | "duplicate-import" | "redundant-import" => {
            delete_lines(source, start, end)
                .map(|e| vec![fix("remove-import", "Remove import", e)])
                .unwrap_or_default()
        }
        "empty-statement" => delete_lines(source, start, end)
            .map(|e| vec![fix("remove-empty-statement", "Remove empty statement", e)])
            .unwrap_or_default(),
        "string-reference-equality" => string_equals(source, start, end)
            .map(|(e, label)| vec![fix("use-equals", &label, e)])
            .unwrap_or_default(),
        "switch-fallthrough" => insert_break(source, start)
            .map(|e| vec![fix("insert-break", "Add the missing `break;`", e)])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Delete the whole line(s) the span sits on, when the span is all that is on them.
///
/// Line-wise rather than span-wise because deleting `import java.util.List;` and leaving its blank
/// line behind is half a fix — the file grows a gap every time. When something else shares the line
/// (`import a.B; import c.D;`, legal if unusual) only the span goes, so the neighbour survives.
fn delete_lines(source: &str, start: usize, end: usize) -> Option<Edit> {
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = source[end..].find('\n').map(|i| end + i + 1).unwrap_or(source.len());
    let before = &source[line_start..start];
    let after = &source[end..line_end];
    let alone = before.trim().is_empty() && after.trim().is_empty();
    let (from, to) = if alone { (line_start, line_end) } else { (start, end) };
    (to > from).then(|| Edit { start: from, end: to, replacement: String::new() })
}

/// `a == b` on strings → `a.equals(b)`; `a != b` → `!a.equals(b)`.
///
/// Reads the operands out of the span rather than re-parsing the file: the check has already decided
/// that both sides are strings, and the span it reports is the comparison. Returns the label too,
/// because the two directions are different repairs and a menu that called them both "Use equals"
/// would hide which one it was about to do.
fn string_equals(source: &str, start: usize, end: usize) -> Option<(Edit, String)> {
    let text = &source[start..end];
    let (op, negated) = if let Some(i) = find_operator(text, "==") {
        (i, false)
    } else {
        (find_operator(text, "!=")?, true)
    };
    let left = text[..op].trim();
    let right = text[op + 2..].trim();
    if left.is_empty() || right.is_empty() {
        return None;
    }
    // `null.equals(x)` would turn a working comparison into a guaranteed NPE. `x == null` is the
    // idiom the check does not flag, but a fix must not depend on that staying true.
    if left == "null" || right == "null" {
        return None;
    }
    // The literal side goes on the left where there is one, which is the null-safe order — the same
    // rule the "flip to null-safe equals" intention applies.
    let (recv, arg) = if right.starts_with('"') && !left.starts_with('"') {
        (right, left)
    } else {
        (left, right)
    };
    let bang = if negated { "!" } else { "" };
    let label = if negated {
        "Replace `!=` with `!equals(…)`".to_string()
    } else {
        "Replace `==` with `equals(…)`".to_string()
    };
    Some((
        Edit { start, end, replacement: format!("{bang}{recv}.equals({arg})") },
        label,
    ))
}

/// The byte index of `op` at the top level of `text` — outside parentheses and string literals.
fn find_operator(text: &str, op: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < text.len() {
        let c = bytes[i];
        match c {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'"' | b'\'' => {
                let quote = c;
                i += 1;
                while i < text.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
            }
            _ if depth == 0 && text[i..].starts_with(op) => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Insert `break;` at the end of the group the fall-through diagnostic points at.
///
/// The diagnostic anchors on the label the control falls INTO, so the `break` belongs on the line
/// above it, indented like the code in the group it ends — a `break` at column zero is a fix that
/// leaves you reformatting.
fn insert_break(source: &str, start: usize) -> Option<Edit> {
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    if line_start == 0 {
        return None; // nothing above to end
    }
    let label_indent: String =
        source[line_start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    // One level in from the `case` label, which is where its statements are.
    let indent = format!("{label_indent}    ");
    Some(Edit {
        start: line_start,
        end: line_start,
        replacement: format!("{indent}break;\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source after applying the single fix `code` offers over `[start, end)`.
    fn applied(code: &str, source: &str, start: usize, end: usize) -> String {
        let fixes = fixes_for(code, source, start, end);
        assert_eq!(fixes.len(), 1, "expected exactly one fix: {fixes:?}");
        let f = &fixes[0];
        format!("{}{}{}", &source[..f.start], f.replacement, &source[f.end..])
    }

    /// The span of the one occurrence of `needle`.
    fn span(source: &str, needle: &str) -> (usize, usize) {
        let s = source.find(needle).expect("needle present");
        (s, s + needle.len())
    }

    #[test]
    fn an_unused_import_takes_its_whole_line() {
        let src = "package p;\nimport java.util.List;\nclass C { }\n";
        let (s, e) = span(src, "import java.util.List;");
        assert_eq!(applied("unused-import", src, s, e), "package p;\nclass C { }\n");
    }

    /// Two imports on one line: only the named one goes.
    #[test]
    fn an_import_sharing_a_line_is_removed_alone() {
        let src = "package p;\nimport a.B; import c.D;\nclass C { }\n";
        let (s, e) = span(src, "import a.B;");
        assert_eq!(applied("unused-import", src, s, e), "package p;\n import c.D;\nclass C { }\n");
    }

    #[test]
    fn a_string_comparison_becomes_equals() {
        let src = "class C { boolean m(String a, String b) { return a == b; } }";
        let (s, e) = span(src, "a == b");
        assert!(applied("string-reference-equality", src, s, e).contains("a.equals(b)"));
    }

    #[test]
    fn a_negated_string_comparison_keeps_its_negation() {
        let src = "class C { boolean m(String a, String b) { return a != b; } }";
        let (s, e) = span(src, "a != b");
        assert!(applied("string-reference-equality", src, s, e).contains("!a.equals(b)"));
    }

    /// The literal goes on the receiver side, which is the order that cannot throw.
    #[test]
    fn a_literal_operand_becomes_the_receiver() {
        let src = "class C { boolean m(String a) { return a == \"x\"; } }";
        let (s, e) = span(src, "a == \"x\"");
        assert!(applied("string-reference-equality", src, s, e).contains("\"x\".equals(a)"));
    }

    /// `x == null` is a working comparison; `null.equals(x)` is a guaranteed NPE.
    #[test]
    fn a_null_comparison_is_not_offered_an_equals() {
        let src = "class C { boolean m(String a) { return a == null; } }";
        let (s, e) = span(src, "a == null");
        assert!(fixes_for("string-reference-equality", src, s, e).is_empty());
    }

    /// An `==` inside a nested call is not the comparison being fixed.
    #[test]
    fn an_operator_inside_parentheses_is_not_the_top_level_one() {
        assert_eq!(find_operator("f(a == b) == c", "=="), Some(10));
    }

    #[test]
    fn a_code_with_no_text_only_fix_offers_nothing() {
        let src = "class C { }";
        assert!(fixes_for("incompatible-type", src, 0, 5).is_empty());
    }

    /// A span that has outlived its text is refused rather than applied to whatever is there now.
    #[test]
    fn an_out_of_range_span_is_refused() {
        assert!(fixes_for("unused-import", "short", 0, 500).is_empty());
    }
}
