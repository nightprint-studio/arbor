//! The Java formatter — re-indentation and whitespace tidying.
//!
//! What it does, precisely: every line is re-indented to its nesting depth, trailing whitespace goes,
//! and runs of blank lines are collapsed. What it deliberately does **not** do: rewrap long lines,
//! reorder anything, insert or remove braces, or normalise spacing inside an expression.
//!
//! That boundary is the design, not a stage of it. A formatter earns trust by being *safe*, and
//! every rule beyond indentation is a rule that can be wrong: `a < b` and `Map<K, V>` differ by
//! context, `-1` and `a - 1` by parse, `x++` and `x + +y` by a space. A formatter that occasionally
//! rewrites an expression is one nobody dares run on a legacy file — which is the only kind of file
//! this product opens. Indentation, by contrast, is derived from structure and cannot change
//! meaning: it is the whole of what Ctrl+Alt+L is actually wanted for on inherited code.
//!
//! ## Text, not a printer
//!
//! It works line by line over a brace/paren depth counter rather than pretty-printing a tree,
//! because it has to work on a file that does not parse. Half-written code is exactly when you
//! reach for the formatter, and a tree-printer's answer to a syntax error is either to bail or to
//! reprint the recovery tree — which reformats code nobody wrote.
//!
//! Literals and comments are tracked as they are scanned, so a brace inside a string never changes
//! the depth and the inside of a block comment is never re-indented.

use crate::Edit;

/// Formatting preferences — the editor's, so a file formats the way it is typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatStyle {
    /// Width of one indent level, in spaces (ignored when `use_tabs`).
    pub indent_width: usize,
    /// Indent with tabs rather than spaces.
    pub use_tabs: bool,
    /// The most consecutive blank lines to keep. `0` removes them all.
    pub max_blank_lines: usize,
    /// Indent the statements under a `case` label one level in from it.
    pub indent_case_body: bool,
}

impl Default for FormatStyle {
    fn default() -> Self {
        Self { indent_width: 4, use_tabs: false, max_blank_lines: 1, indent_case_body: true }
    }
}

/// The edits that reformat `source`, or an empty list when it is already formatted.
///
/// Line-range edits rather than one whole-document replacement: a formatter that touched three
/// lines should mark three lines changed, keep the caret where it was, and leave the rest of the
/// undo history meaning what it meant. They are returned in **descending** start order, so a caller
/// applies them in sequence without remapping anything.
pub fn format_edits(source: &str, style: FormatStyle) -> Vec<Edit> {
    let formatted = format_source(source, style);
    if formatted == source {
        return Vec::new();
    }
    diff_lines(source, &formatted)
}

/// `source`, reformatted. Exposed for tests and for a caller that wants the text rather than edits.
pub fn format_source(source: &str, style: FormatStyle) -> String {
    // The line ending the file uses, preserved rather than imposed: normalising it would rewrite
    // every line of every file in a Windows checkout.
    let newline = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing_newline = source.ends_with('\n');

    let mut lines: Vec<String> = Vec::new();
    let mut state = ScanState::default();
    let mut depth: i32 = 0;
    // How many blank lines have been kept since the last line with something on it.
    let mut blanks = 0usize;

    for raw in source.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim();

        // Inside a block comment or a text block the content is the author's — only the trailing
        // whitespace goes, and the indentation is left exactly as written. A comment's alignment is
        // often deliberate; a text block's IS the string.
        if state.in_block_comment || state.in_text_block {
            lines.push(line.trim_end().to_string());
            state.scan(line);
            blanks = 0;
            continue;
        }

        if trimmed.is_empty() {
            blanks += 1;
            if blanks <= style.max_blank_lines {
                lines.push(String::new());
            }
            continue;
        }
        blanks = 0;

        // A line that STARTS by closing what a previous one opened is drawn at the outer level, so
        // `}` lines up with the `if` rather than with its body. A `case` label is the same idea for
        // a level braces do not mark: it sits at the switch's, and its statements one in — which is
        // also what stops a run of labels stair-stepping ever deeper, since nothing is counted.
        let outdent = starts_with_closer(trimmed) || (style.indent_case_body && is_case_label(trimmed));
        let level = if outdent { (depth - 1).max(0) } else { depth };
        lines.push(format!("{}{}", indent_of(level as usize, style), trimmed));

        depth = (depth + delta_of(line, &mut state)).max(0);
    }

    // `split('\n')` on a file ending in a newline yields a final empty piece, which is that
    // newline. Rebuild it explicitly rather than let the join lose or duplicate it.
    if trailing_newline {
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        let mut out = lines.join(newline);
        out.push_str(newline);
        out
    } else {
        lines.join(newline)
    }
}

/// One indent level, repeated.
fn indent_of(level: usize, style: FormatStyle) -> String {
    if style.use_tabs {
        "\t".repeat(level)
    } else {
        " ".repeat(level * style.indent_width)
    }
}

/// Whether a line opens by closing a block — `}`, `)`, `]`, or an `} else {`.
fn starts_with_closer(trimmed: &str) -> bool {
    trimmed.starts_with('}') || trimmed.starts_with(')') || trimmed.starts_with(']')
}

/// Whether a line is a `case` / `default` label.
///
/// `default` needs its `:` or `->`: a bare `default ` also begins an interface's default **method**,
/// and outdenting one of those would draw a method body outside its interface.
fn is_case_label(trimmed: &str) -> bool {
    trimmed.starts_with("case ")
        || trimmed.starts_with("default:")
        || trimmed.starts_with("default ->")
}

/// What the scanner needs to remember between lines.
#[derive(Default)]
struct ScanState {
    in_block_comment: bool,
    in_text_block: bool,
}

impl ScanState {
    /// Advance the state over `line` without measuring depth — for a line whose content is not code.
    fn scan(&mut self, line: &str) {
        let mut depth = 0i32;
        self.measure(line, &mut depth);
    }

    /// Walk `line`, updating the state and adding each code brace to `depth`.
    ///
    /// One pass, character by character, because the things that must not count — a brace in a
    /// string, in a char literal, in a comment, in a text block — are only knowable by having read
    /// everything before them on the line.
    fn measure(&mut self, line: &str, depth: &mut i32) {
        let b = line.as_bytes();
        let mut i = 0usize;
        while i < b.len() {
            if self.in_block_comment {
                if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    self.in_block_comment = false;
                    i += 2;
                    continue;
                }
                i += 1;
                continue;
            }
            if self.in_text_block {
                if line[i..].starts_with("\"\"\"") {
                    self.in_text_block = false;
                    i += 3;
                    continue;
                }
                i += 1;
                continue;
            }
            if line[i..].starts_with("//") {
                return; // the rest of the line is a comment
            }
            if line[i..].starts_with("/*") {
                self.in_block_comment = true;
                i += 2;
                continue;
            }
            if line[i..].starts_with("\"\"\"") {
                self.in_text_block = true;
                i += 3;
                continue;
            }
            match b[i] {
                b'"' | b'\'' => {
                    let quote = b[i];
                    i += 1;
                    while i < b.len() {
                        if b[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == quote {
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                }
                b'{' => {
                    *depth += 1;
                    i += 1;
                }
                b'}' => {
                    *depth -= 1;
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }
}

/// The net brace depth `line` adds, advancing `state` over it.
fn delta_of(line: &str, state: &mut ScanState) -> i32 {
    let mut depth = 0i32;
    state.measure(line, &mut depth);
    depth
}

/// The minimal per-line edits turning `before` into `after`.
///
/// Both have the same line count by construction *except* where blank lines were collapsed, so this
/// walks them together and emits a replacement for each contiguous run that differs. Descending
/// order, so a caller applies them without remapping.
fn diff_lines(before: &str, after: &str) -> Vec<Edit> {
    let before_lines: Vec<&str> = before.split_inclusive('\n').collect();
    let after_lines: Vec<&str> = after.split_inclusive('\n').collect();

    // A line count that changed means blank lines went; matching them up line by line would emit an
    // edit for every line after the first collapse. One whole-document edit is both smaller and
    // honest about what happened.
    if before_lines.len() != after_lines.len() {
        return vec![Edit { start: 0, end: before.len(), replacement: after.to_string() }];
    }

    let mut edits: Vec<Edit> = Vec::new();
    let mut offset = 0usize;
    for (b, a) in before_lines.iter().zip(after_lines.iter()) {
        if b != a {
            edits.push(Edit {
                start: offset,
                end: offset + b.len(),
                replacement: (*a).to_string(),
            });
        }
        offset += b.len();
    }
    edits.reverse();
    edits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format_source(src, FormatStyle::default())
    }

    #[test]
    fn a_body_is_indented_one_level_per_brace() {
        let src = "class C {\nvoid m() {\nint x = 1;\n}\n}\n";
        assert_eq!(fmt(src), "class C {\n    void m() {\n        int x = 1;\n    }\n}\n");
    }

    #[test]
    fn a_closing_brace_lines_up_with_what_opened_it() {
        let src = "class C {\n    void m() {\n        if (x) {\n        y();\n            }\n    }\n}\n";
        let out = fmt(src);
        assert!(out.contains("        }\n"), "{out}");
    }

    #[test]
    fn trailing_whitespace_goes() {
        assert_eq!(fmt("class C {   \n}\n"), "class C {\n}\n");
    }

    #[test]
    fn a_run_of_blank_lines_is_collapsed_to_the_limit() {
        let src = "class C {\n\n\n\n    void m() {}\n}\n";
        assert_eq!(fmt(src), "class C {\n\n    void m() {}\n}\n");
    }

    /// A brace inside a string is not a brace.
    #[test]
    fn a_brace_in_a_string_does_not_open_a_level() {
        let src = "class C {\nString s = \"{\";\nint x = 1;\n}\n";
        let out = fmt(src);
        assert!(out.contains("    int x = 1;"), "{out}");
    }

    /// Nor is one in a char literal, nor in a line comment.
    #[test]
    fn a_brace_in_a_comment_or_char_literal_does_not_open_a_level() {
        let src = "class C {\nchar c = '{';\n// {\nint x = 1;\n}\n";
        let out = fmt(src);
        assert!(out.contains("    int x = 1;"), "{out}");
    }

    /// The inside of a block comment is the author's — indentation included, since a comment's
    /// alignment is often deliberate (an ASCII diagram, a wrapped sentence).
    #[test]
    fn a_block_comments_interior_is_left_alone() {
        let src = "class C {\n/*\n      aligned\n*/\nint x = 1;\n}\n";
        let out = fmt(src);
        assert!(out.contains("      aligned"), "{out}");
    }

    /// A text block's content is data — re-indenting it would change the string.
    #[test]
    fn a_text_blocks_content_is_left_alone() {
        let src = "class C {\nString s = \"\"\"\n      keep me\n      \"\"\";\n}\n";
        let out = fmt(src);
        assert!(out.contains("      keep me"), "{out}");
    }

    #[test]
    fn a_case_label_sits_at_the_switch_level() {
        let src = "class C {\nvoid m() {\nswitch (x) {\ncase 1:\ndo();\nbreak;\n}\n}\n}\n";
        let out = fmt(src);
        assert!(out.contains("        case 1:\n"), "{out}");
        assert!(out.contains("            do();\n"), "{out}");
    }

    /// Already-formatted input produces no edits at all — which is what lets the editor say
    /// "already formatted" rather than marking the file dirty for nothing.
    #[test]
    fn formatted_input_produces_no_edits() {
        let src = "class C {\n    void m() {\n        int x = 1;\n    }\n}\n";
        assert!(format_edits(src, FormatStyle::default()).is_empty());
    }

    #[test]
    fn tabs_are_used_when_asked_for() {
        let style = FormatStyle { use_tabs: true, ..FormatStyle::default() };
        let out = format_source("class C {\nvoid m() {}\n}\n", style);
        assert!(out.contains("\tvoid m() {}"), "{out}");
    }

    /// Applying the edits must reproduce the formatted text exactly — they are the only thing the
    /// editor ever sees.
    #[test]
    fn the_edits_reproduce_the_formatted_source() {
        let src = "class C {\nvoid m() {\nint x = 1;\n}\n}\n";
        let mut applied = src.to_string();
        for e in format_edits(src, FormatStyle::default()) {
            applied.replace_range(e.start..e.end, &e.replacement);
        }
        assert_eq!(applied, fmt(src));
    }

    /// A file that does not parse is exactly when the formatter is reached for; it must still work.
    #[test]
    fn a_file_with_a_syntax_error_still_formats() {
        let src = "class C {\nvoid m( {\nint x = 1;\n}\n}\n";
        let out = fmt(src);
        assert!(out.contains("    void m( {"), "{out}");
    }

    /// CRLF in, CRLF out — a formatter that normalised line endings would rewrite every line of
    /// every file in a Windows checkout.
    #[test]
    fn windows_line_endings_survive() {
        let out = format_source("class C {\r\nvoid m() {}\r\n}\r\n", FormatStyle::default());
        assert!(out.contains("\r\n    void m() {}"), "{out:?}");
        assert!(!out.contains("\n\n"), "no bare newlines introduced: {out:?}");
    }
}
