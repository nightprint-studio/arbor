//! Commented-out code, taken out of what the model reads.
//!
//! A model that completes code reads the code above the caret, comments included, and a block of old code in `//` lines
//! is a very strong hint: it copies the line, element by element, into whatever is being written under it. That helps
//! when the old version is what is wanted and misleads when it was commented out because it was wrong — and it makes a
//! test of the model say more about the comment than about the model. This removes such blocks, and only them: prose
//! comments, Javadoc and a single `// TODO` stay.

/// `text` without the runs of `//` lines that are code. A run is code when one of its lines ends the way a statement
/// does (`;`, `{`, `}`) or is a call (`name(…)`) — and the whole run goes, so the lines of a statement spread over
/// several comment lines go together.
pub fn strip_commented_code(text: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < lines.len() {
        if comment_body(lines[i]).is_none() {
            out.push_str(lines[i]);
            i += 1;
            continue;
        }
        let end = (i..lines.len()).find(|&j| comment_body(lines[j]).is_none()).unwrap_or(lines.len());
        if !lines[i..end].iter().filter_map(|l| comment_body(l)).any(looks_like_code) {
            lines[i..end].iter().for_each(|l| out.push_str(l));
        }
        i = end;
    }
    out
}

/// What follows the `//` of a whole-line comment; `None` for any other line and for a `///` doc line.
fn comment_body(line: &str) -> Option<&str> {
    let body = line.trim_start().strip_prefix("//")?;
    (!body.starts_with('/')).then(|| body.trim())
}

fn looks_like_code(body: &str) -> bool {
    if body.ends_with([';', '{', '}']) {
        return true;
    }
    // `name(…)` or `a.b(…)` as the whole line: a call, not a sentence that mentions one
    let Some(open) = body.find('(') else { return false };
    let callee = &body[..open];
    !callee.is_empty() && callee.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '$')) && body.ends_with(')')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_of_old_code_goes_whole() {
        let src = "a();\n// Collections.addAll(\n//     tabellati\n//     , Pair.of(x)\n// );\nb();\n";
        assert_eq!(strip_commented_code(src), "a();\nb();\n");
    }

    #[test]
    fn prose_and_javadoc_stay() {
        let src = "// the retry count is read once at start\n/// Docs for `foo()`.\nint n;\n";
        assert_eq!(strip_commented_code(src), src);
    }

    #[test]
    fn a_single_commented_statement_goes_and_a_todo_stays() {
        assert_eq!(strip_commented_code("// repo.save(x);\n// TODO later\nz();\n"), "z();\n");
    }

    #[test]
    fn a_prose_line_that_mentions_a_call_stays() {
        let src = "// calls repo.save(x) once per batch\nz();\n";
        assert_eq!(strip_commented_code(src), src);
    }

    #[test]
    fn code_is_untouched_and_a_trailing_comment_stays() {
        let src = "int a = 1; // one;\nint b = 2;\n";
        assert_eq!(strip_commented_code(src), src);
    }
}
