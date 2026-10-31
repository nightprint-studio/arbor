//! How a project lays out its code, measured from the code itself.
//!
//! Formatting is the one thing about a style that needs no guessing: a file shows its indent, its brace placement, its
//! spacing, and a project is the sum of its files. [`FormatCounts::of`] counts what one file does, counts add, and
//! [`FormatStyle::from`] says what the project *does* only where it does it nearly always — a style that is half and half
//! is not a style, so nothing is imposed there.
//!
//! [`conform`] applies it to a piece of suggested text, and touches only what is measured: `if(` becomes `if (` in a
//! project that always writes the space, and is left alone where the project is mixed. Text inside string and character
//! literals is never touched.

use serde::{Deserialize, Serialize};

/// Fewer sightings than this in a pair and the project has no style on it.
const MIN_SEEN: u32 = 30;
/// A way is the project's when it holds at least this share of the pair.
const MIN_SHARE: f32 = 0.8;

const KEYWORDS: [&str; 6] = ["if", "for", "while", "switch", "catch", "synchronized"];

/// What one file (or a project, summed) does, as plain counters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormatCounts {
    /// Lines indented with a tab.
    pub tab_lines: u32,
    /// How often a line was indented 2, 4 and 8 spaces deeper than the line before it.
    pub step2: u32,
    pub step4: u32,
    pub step8: u32,
    /// A `{` that ends a line with code on it, against a `{` alone on its line.
    pub brace_same_line: u32,
    pub brace_next_line: u32,
    /// `if (` against `if(`, for every control keyword.
    pub keyword_space: u32,
    pub keyword_tight: u32,
    /// `) {` against `){`.
    pub brace_space: u32,
    pub brace_tight: u32,
    /// `, x` against `,x`.
    pub comma_space: u32,
    pub comma_tight: u32,
    /// A chain broken before its dot (`.map(…)` starts the line) against after it (the line ends with `.`).
    pub dot_leads: u32,
    pub dot_trails: u32,
}

/// How a project indents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indent {
    Tabs,
    Spaces(u8),
}

/// The ways a project lays out its code that it follows nearly always; `None` where it is mixed or unseen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormatStyle {
    pub indent: Option<Indent>,
    /// `true`: `if (x) {` on one line; `false`: the brace on its own line.
    pub brace_on_same_line: Option<bool>,
    /// `true`: `if (`; `false`: `if(`.
    pub space_after_keyword: Option<bool>,
    /// `true`: `) {`; `false`: `){`.
    pub space_before_brace: Option<bool>,
    /// `true`: `a, b`; `false`: `a,b`.
    pub space_after_comma: Option<bool>,
    /// `true`: the dot starts the continuation line.
    pub dot_leads_line: Option<bool>,
}

impl FormatCounts {
    /// What `source` does.
    pub fn of(source: &str) -> Self {
        let mut c = FormatCounts::default();
        let mut previous_indent: Option<usize> = None;
        let mut in_block_comment = false;
        for line in source.lines() {
            let trimmed = line.trim();
            if in_block_comment {
                in_block_comment = !trimmed.contains("*/");
                continue;
            }
            if trimmed.starts_with("/*") {
                in_block_comment = !trimmed.contains("*/");
                continue;
            }
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            c.indent_of(line, &mut previous_indent);
            c.layout_of(trimmed);
        }
        c
    }

    fn indent_of(&mut self, line: &str, previous: &mut Option<usize>) {
        if line.starts_with('\t') {
            self.tab_lines += 1;
            *previous = None;
            return;
        }
        let indent = line.len() - line.trim_start_matches(' ').len();
        if let Some(before) = *previous {
            match indent.checked_sub(before) {
                Some(2) => self.step2 += 1,
                Some(4) => self.step4 += 1,
                Some(8) => self.step8 += 1,
                _ => {}
            }
        }
        *previous = Some(indent);
    }

    fn layout_of(&mut self, trimmed: &str) {
        if trimmed == "{" {
            self.brace_next_line += 1;
        } else if trimmed.ends_with('{') {
            self.brace_same_line += 1;
        }
        if trimmed.starts_with('.') && !trimmed.starts_with("..") {
            self.dot_leads += 1;
        } else if trimmed.ends_with('.') && !trimmed.ends_with("..") {
            self.dot_trails += 1;
        }
        let code = code_only(trimmed);
        let bytes = code.as_bytes();
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b',' => match bytes.get(i + 1) {
                    Some(b' ') => self.comma_space += 1,
                    Some(b) if !b.is_ascii_whitespace() && *b != b')' => self.comma_tight += 1,
                    _ => {}
                },
                b'{' if i >= 1 => match (bytes[i - 1], i >= 2 && bytes[i - 2] == b')') {
                    (b')', _) => self.brace_tight += 1,
                    (b' ', true) => self.brace_space += 1,
                    _ => {}
                },
                _ => {}
            }
        }
        for keyword in KEYWORDS {
            let mut from = 0;
            while let Some(found) = code[from..].find(keyword) {
                let at = from + found;
                let (before, after) = (at.checked_sub(1).map(|p| bytes[p]), bytes.get(at + keyword.len()).copied());
                from = at + keyword.len();
                if before.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.') {
                    continue;
                }
                match after {
                    Some(b' ') if bytes.get(at + keyword.len() + 1) == Some(&b'(') => self.keyword_space += 1,
                    Some(b'(') => self.keyword_tight += 1,
                    _ => {}
                }
            }
        }
    }

    /// Add what `other` counted.
    pub fn merge(&mut self, other: &FormatCounts) {
        macro_rules! add {
            ($($f:ident),*) => { $(self.$f += other.$f;)* };
        }
        add!(
            tab_lines, step2, step4, step8, brace_same_line, brace_next_line, keyword_space, keyword_tight, brace_space,
            brace_tight, comma_space, comma_tight, dot_leads, dot_trails
        );
    }
}

impl From<&FormatCounts> for FormatStyle {
    fn from(c: &FormatCounts) -> Self {
        let indent = {
            let spaces = c.step2 + c.step4 + c.step8;
            if c.tab_lines >= MIN_SEEN && c.tab_lines as f32 >= MIN_SHARE * (c.tab_lines + spaces) as f32 {
                Some(Indent::Tabs)
            } else if spaces >= MIN_SEEN {
                [(2u8, c.step2), (4, c.step4), (8, c.step8)]
                    .into_iter()
                    .find(|(_, n)| *n as f32 >= MIN_SHARE * spaces as f32)
                    .map(|(width, _)| Indent::Spaces(width))
            } else {
                None
            }
        };
        FormatStyle {
            indent,
            brace_on_same_line: decided(c.brace_same_line, c.brace_next_line),
            space_after_keyword: decided(c.keyword_space, c.keyword_tight),
            space_before_brace: decided(c.brace_space, c.brace_tight),
            space_after_comma: decided(c.comma_space, c.comma_tight),
            dot_leads_line: decided(c.dot_leads, c.dot_trails),
        }
    }
}

/// `Some(true)` when the first way holds the share, `Some(false)` when the second does, else `None`.
fn decided(first: u32, second: u32) -> Option<bool> {
    let total = first + second;
    if total < MIN_SEEN {
        return None;
    }
    let share = first as f32 / total as f32;
    if share >= MIN_SHARE {
        Some(true)
    } else if 1.0 - share >= MIN_SHARE {
        Some(false)
    } else {
        None
    }
}

/// `line` with the contents of string and character literals blanked, so a `,` or an `if(` inside one is not counted.
fn code_only(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in line.chars() {
        match quote {
            Some(q) => {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
                out.push(if c == q { c } else { '_' });
            }
            None => {
                if c == '"' || c == '\'' {
                    quote = Some(c);
                }
                out.push(c);
            }
        }
    }
    out
}

/// `text` laid out the way `style` says, where it says: the space after a control keyword, before a block's brace and
/// after a comma. Literals are left exactly as they are, and a way the project is mixed on is not imposed.
pub fn conform(text: &str, style: &FormatStyle) -> String {
    let masked = code_only(text);
    let (src, mask) = (text.as_bytes(), masked.as_bytes());
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < src.len() {
        // a literal's contents were blanked to `_` in the mask, and are copied from the original
        let in_literal = src[i] != mask[i];
        let c = text[i..].chars().next().unwrap_or(' ');
        let len = c.len_utf8();
        let boundary = i == 0 || !(mask[i - 1].is_ascii_alphanumeric() || mask[i - 1] == b'_' || mask[i - 1] == b'.');
        if !in_literal && boundary {
            if let Some(kw) = KEYWORDS.iter().find(|k| mask[i..].starts_with(k.as_bytes())) {
                let end = i + kw.len();
                let tight = mask.get(end) == Some(&b'(');
                let spaced = mask.get(end) == Some(&b' ') && mask.get(end + 1) == Some(&b'(');
                if (style.space_after_keyword == Some(true) && tight) || (style.space_after_keyword == Some(false) && spaced) {
                    out.push_str(kw);
                    if tight {
                        out.push(' ');
                        i = end;
                    } else {
                        i = end + 1;
                    }
                    continue;
                }
            }
        }
        if !in_literal && c == ')' && mask.get(i + 1) == Some(&b'{') && style.space_before_brace == Some(true) {
            out.push_str(") ");
            i += 1;
            continue;
        }
        if !in_literal && c == ')' && mask.get(i + 1) == Some(&b' ') && mask.get(i + 2) == Some(&b'{') && style.space_before_brace == Some(false) {
            out.push(')');
            i += 2;
            continue;
        }
        if !in_literal && c == ',' && style.space_after_comma == Some(true) && mask.get(i + 1).is_some_and(|b| !b.is_ascii_whitespace() && *b != b')') {
            out.push_str(", ");
            i += 1;
            continue;
        }
        out.push(c);
        i += len;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn four_space_project() -> FormatCounts {
        let file = "class A {\n    void f() {\n        if (x) {\n            call(a, b);\n        }\n    }\n}\n";
        let mut all = FormatCounts::default();
        for _ in 0..40 {
            all.merge(&FormatCounts::of(file));
        }
        all
    }

    #[test]
    fn a_project_that_always_does_it_one_way_has_that_style() {
        let style = FormatStyle::from(&four_space_project());
        assert_eq!(style.indent, Some(Indent::Spaces(4)));
        assert_eq!(style.brace_on_same_line, Some(true));
        assert_eq!(style.space_after_keyword, Some(true));
        assert_eq!(style.space_before_brace, Some(true));
        assert_eq!(style.space_after_comma, Some(true));
    }

    #[test]
    fn a_mixed_or_unseen_way_is_not_a_style() {
        assert_eq!(FormatStyle::from(&FormatCounts::default()), FormatStyle::default());
        let mixed = FormatCounts { keyword_space: 20, keyword_tight: 20, ..Default::default() };
        assert_eq!(FormatStyle::from(&mixed).space_after_keyword, None);
    }

    #[test]
    fn tabs_and_the_brace_on_its_own_line_are_read() {
        let file = "class A\n{\n\tvoid f()\n\t{\n\t\tif(x)\n\t\t{\n\t\t}\n\t}\n}\n";
        let mut all = FormatCounts::default();
        for _ in 0..40 {
            all.merge(&FormatCounts::of(file));
        }
        let style = FormatStyle::from(&all);
        assert_eq!(style.indent, Some(Indent::Tabs));
        assert_eq!(style.brace_on_same_line, Some(false));
        assert_eq!(style.space_after_keyword, Some(false));
    }

    #[test]
    fn comments_and_literals_are_not_counted() {
        let c = FormatCounts::of("// if(x) a,b\n/* while(y)\n c,d */\nString s = \"if(a,b){\";\n");
        assert_eq!((c.keyword_tight, c.comma_tight, c.brace_tight), (0, 0, 0));
    }

    #[test]
    fn a_leading_dot_and_a_trailing_one_are_told_apart() {
        let c = FormatCounts::of("x\n    .map(a)\n    .filter(b)\ny.\n  z();\n");
        assert_eq!((c.dot_leads, c.dot_trails), (2, 1));
    }

    #[test]
    fn text_is_conformed_only_where_the_style_says() {
        let style = FormatStyle::from(&four_space_project());
        assert_eq!(conform("if(a){ f(a,b); }", &style), "if (a) { f(a, b); }");
        let tight = FormatStyle { space_after_keyword: Some(false), ..Default::default() };
        assert_eq!(conform("if (a) { f(a,b); }", &tight), "if(a) { f(a,b); }");
        assert_eq!(conform("if(a){ f(a,b); }", &FormatStyle::default()), "if(a){ f(a,b); }");
    }

    #[test]
    fn literals_and_longer_words_are_left_alone() {
        let style = FormatStyle::from(&four_space_project());
        assert_eq!(conform("log(\"if(a,b){\", notif(x), a.for(y))", &style), "log(\"if(a,b){\", notif(x), a.for(y))");
    }
}
