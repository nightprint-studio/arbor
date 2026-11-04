//! Block comments and Javadoc: keeping their continuation lines in step with the code they annotate.
//!
//! The comment's *words* are never touched except by `wrap_javadoc`, which only ever breaks a long line
//! at a space — it joins nothing, so running it twice changes nothing the second time. Whitespace
//! before a line's `*` (or, in a comment without stars, before its text) is what gets rewritten.

use super::layout::{indent_string, text_width};
use super::style::FormatStyle;

/// `text` (a block comment that spans lines) re-indented for a comment starting at column `start_col`.
/// `orig_col` is the column it started at in the source, when it began a line — the reference for
/// shifting a comment without stars as a block.
pub(crate) fn realign(text: &str, start_col: usize, orig_col: Option<usize>, style: &FormatStyle) -> String {
    let iw = style.indent_width.max(1);
    let javadoc = text.starts_with("/**");
    let mut lines = text.split('\n');
    let mut out = lines.next().unwrap_or("").trim_end().to_string();
    let delta = start_col as isize - orig_col.unwrap_or(start_col) as isize;
    let mut in_pre = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if trimmed.starts_with('*') {
            let body = trimmed.trim_start_matches('*');
            let is_close = trimmed == "*/" || trimmed.ends_with("*/") && body.trim() == "/";
            let prefix = format!("{} *", indent_string(start_col, style));
            if is_close {
                out.push_str(&format!("\n{prefix}/"));
                continue;
            }
            // `*` followed by text: keep the author's single space (or lack of one) after the star.
            let content = &trimmed[1..];
            if javadoc && style.wrap_javadoc && style.line_width > 0 {
                if content.contains("<pre") {
                    in_pre = true;
                }
                if !in_pre {
                    for piece in wrap_line(content, start_col + 2, style.line_width) {
                        out.push_str(&format!("\n{prefix}{piece}"));
                    }
                    continue;
                }
                if content.contains("</pre") {
                    in_pre = false;
                }
            }
            out.push_str(&format!("\n{prefix}{content}"));
        } else {
            let old = text_width(&line[..line.len() - line.trim_start().len()], iw) as isize;
            let new = (old + delta).max(0) as usize;
            out.push_str(&format!("\n{}{}", indent_string(new, style), trimmed));
        }
    }
    out
}

/// Break `content` (the text after a comment line's `*`, leading space included) so that
/// `prefix_cols + content` fits `width`, at spaces outside `{@…}`. A line already short enough comes
/// back unchanged; a continuation of an `@tag` line is indented four more columns.
fn wrap_line(content: &str, prefix_cols: usize, width: usize) -> Vec<String> {
    let tag = content.trim_start().starts_with('@');
    let mut out: Vec<String> = Vec::new();
    let mut rest = content.to_string();
    loop {
        if prefix_cols + rest.chars().count() <= width {
            out.push(rest);
            return out;
        }
        let limit = width.saturating_sub(prefix_cols).max(1);
        let chars: Vec<char> = rest.chars().collect();
        let mut cut: Option<usize> = None;
        let mut depth = 0i32;
        for (i, &c) in chars.iter().enumerate() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                ' ' if i > 0 && i <= limit && depth <= 0 && chars[..i].iter().any(|ch| *ch != ' ') => cut = Some(i),
                _ => {}
            }
        }
        // Nothing to break at within the margin: leave the line, however long.
        let Some(at) = cut else {
            out.push(rest);
            return out;
        };
        let head: String = chars[..at].iter().collect();
        let tail: String = chars[at + 1..].iter().collect();
        out.push(head.trim_end().to_string());
        let lead = if tag { "     " } else { " " };
        rest = format!("{lead}{}", tail.trim_start());
        if tail.trim().is_empty() {
            return out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_lines_line_up_under_the_opening_slash() {
        let style = FormatStyle::default();
        let got = realign("/**\n     * Doc.\n   */", 4, Some(0), &style);
        assert_eq!(got, "/**\n     * Doc.\n     */");
    }

    #[test]
    fn a_long_javadoc_line_is_broken_at_a_space_and_stays_broken() {
        let style = FormatStyle { wrap_javadoc: true, line_width: 30, ..FormatStyle::default() };
        let once = realign("/**\n * aaaa bbbb cccc dddd eeee ffff gggg hhhh\n */", 0, Some(0), &style);
        assert!(once.lines().all(|l| l.chars().count() <= 30), "{once}");
        assert_eq!(realign(&once, 0, Some(0), &style), once);
    }

    #[test]
    fn a_comment_without_stars_moves_as_a_block() {
        let style = FormatStyle::default();
        let got = realign("/* a\n        b */", 4, Some(8), &style);
        assert_eq!(got, "/* a\n    b */");
    }
}
