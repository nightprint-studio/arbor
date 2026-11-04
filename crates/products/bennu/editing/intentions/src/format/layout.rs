//! Turning tokens and line-break flags into text: where each line is indented, what its text is, and
//! how wide it comes out.
//!
//! Indentation is not a function of brace depth alone, because a wrapped line is indented relative to
//! the line that *started* the statement, a case body relative to its label, and an argument relative
//! to the `(` it follows. [`Machine`] walks the lines in order, keeping one [`Scope`] per open bracket
//! (the indent of the line that opened it, and where its contents are anchored) and the indent of the
//! current statement, and answers "how far in does this line go" for each. It is `Clone`, which is how
//! the wrapping pass tries a layout without committing to it.

use super::classify::{next_sig, prev_sig, BraceKind, Info, NO_MATCH};
use super::lex::{Kind, Tok};
use super::spacing::Gap;
use super::style::FormatStyle;

/// Everything a layout decision reads that is not a token.
pub(crate) struct Ctx<'a> {
    pub info: &'a [Info],
    pub gaps: &'a [Gap],
    pub style: &'a FormatStyle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScopeKind {
    Paren,
    Brace(BraceKind),
}

#[derive(Clone, Debug)]
struct Scope {
    kind: ScopeKind,
    /// The indent contents are measured from.
    anchor: usize,
    /// The column just past an opening `(` — where an aligned argument list lines up.
    align: usize,
    /// A `switch` scope that has seen a `case` label (its statements sit one level in from it).
    in_case: bool,
}

/// One laid-out line.
#[derive(Clone, Debug)]
pub(crate) struct LineOut {
    pub indent: usize,
    /// The line without its indentation.
    pub text: String,
    /// Indent plus the width of the text (of its last physical line, for a multi-line comment).
    pub width: usize,
}

#[derive(Clone)]
pub(crate) struct Machine {
    stack: Vec<Scope>,
    base: usize,
    stmt_indent: usize,
    prev_last: Option<usize>,
    prev_annotations_only: bool,
    pending_body: bool,
    cur_indent: usize,
}

pub(crate) fn text_width(s: &str, tab: usize) -> usize {
    let last = s.rsplit('\n').next().unwrap_or("");
    last.chars().map(|c| if c == '\t' { tab } else { 1 }).sum()
}

fn is_closer(t: &Tok) -> bool {
    t.kind == Kind::Punct && matches!(t.text.as_str(), "}" | ")" | "]")
}

/// `@Override`, `@SuppressWarnings("x") @Deprecated` — a line of nothing but annotations.
fn only_annotations(toks: &[Tok], info: &[Info], first: usize, last: usize) -> bool {
    let mut i = first;
    let mut seen = false;
    while i <= last {
        if !(toks[i].is("@") && i < last && toks[i + 1].is_word() && toks[i + 1].text != "interface") {
            return false;
        }
        seen = true;
        i += 2;
        while i + 1 <= last && toks[i].is(".") && toks[i + 1].is_word() {
            i += 2;
        }
        if i <= last && toks[i].is("(") {
            let m = info[i].matching;
            if m == NO_MATCH || m > last {
                return false;
            }
            i = m + 1;
        }
    }
    seen
}

impl Machine {
    pub fn new(base: usize) -> Self {
        Self {
            stack: Vec::new(),
            base,
            stmt_indent: base,
            prev_last: None,
            prev_annotations_only: false,
            pending_body: false,
            cur_indent: base,
        }
    }

    fn content(&self, ctx: &Ctx) -> usize {
        let iw = ctx.style.indent_width.max(1);
        match self.stack.last() {
            None => self.base,
            Some(s) => match s.kind {
                ScopeKind::Paren => {
                    if ctx.style.align_wrapped_to_paren {
                        s.align
                    } else {
                        s.anchor + ctx.style.continuation_indent
                    }
                }
                ScopeKind::Brace(BraceKind::Array) => s.anchor + iw,
                ScopeKind::Brace(BraceKind::Switch) => {
                    s.anchor + iw + if s.in_case && ctx.style.indent_case_body { iw } else { 0 }
                }
                ScopeKind::Brace(_) => s.anchor + iw,
            },
        }
    }

    fn in_element_scope(&self) -> bool {
        matches!(self.stack.last().map(|s| s.kind), Some(ScopeKind::Paren) | Some(ScopeKind::Brace(BraceKind::Array)))
    }

    fn is_statement_start(&self, toks: &[Tok], ctx: &Ctx) -> bool {
        if self.prev_annotations_only {
            return true;
        }
        let Some(p) = self.prev_last else { return true };
        let t = &toks[p];
        match t.text.as_str() {
            ";" => true,
            "{" => true,
            "}" => matches!(
                ctx.info[p].brace,
                Some(BraceKind::Type) | Some(BraceKind::Method) | Some(BraceKind::Block) | Some(BraceKind::Switch)
            ),
            ":" => ctx.info[p].role == super::classify::Role::LabelColon,
            "," => matches!(self.stack.last().map(|s| s.kind), Some(ScopeKind::Brace(BraceKind::Type))),
            _ => false,
        }
    }

    /// Lay out the line `first..=last`: compute its indent, build its text, and advance the machine.
    pub fn layout_line(&mut self, toks: &[Tok], ctx: &Ctx, first: usize, last: usize) -> LineOut {
        let style = ctx.style;
        let iw = style.indent_width.max(1);
        let cont = style.continuation_indent;
        let ft = &toks[first];

        let comment_only = toks[first..=last].iter().all(Tok::is_comment);
        let case_label = !comment_only
            && (ft.is("case") || (ft.is("default") && next_sig(toks, first).is_some_and(|n| toks[n].is(":") || toks[n].is("->"))));
        let in_switch = matches!(self.stack.last().map(|s| s.kind), Some(ScopeKind::Brace(BraceKind::Switch)));

        let (indent, starts_statement) = if is_closer(ft) {
            (self.stack.last().map_or(self.base, |s| s.anchor), true)
        } else if comment_only {
            let at = if self.pending_body { self.stmt_indent + iw } else { self.content(ctx) };
            (at, false)
        } else if case_label && in_switch {
            (self.stack.last().map_or(self.base, |s| s.anchor + iw), true)
        } else if self.in_element_scope() {
            let base = self.content(ctx);
            let elem_start = match self.prev_last {
                None => true,
                Some(p) => {
                    let pt = &toks[p];
                    pt.is("(") || pt.is("[") || pt.is(",") || (pt.is("{") && ctx.info[p].brace == Some(BraceKind::Array))
                }
            } || self.prev_annotations_only;
            (if elem_start { base } else { base + cont }, elem_start)
        } else {
            let statement = self.is_statement_start(toks, ctx);
            let own_brace = ft.is("{")
                && matches!(
                    ctx.info[first].brace,
                    Some(BraceKind::Type) | Some(BraceKind::Method) | Some(BraceKind::Block) | Some(BraceKind::Switch)
                );
            if own_brace && !statement {
                // Allman: the brace sits at the indent of the statement it opens.
                (self.stmt_indent, true)
            } else if self.pending_body {
                // The unbraced body of an `if` / `else` / `for` / `while`.
                (self.stmt_indent + iw, true)
            } else if statement {
                (self.content(ctx), true)
            } else {
                (self.stmt_indent + cont, false)
            }
        };

        self.cur_indent = indent;
        if starts_statement {
            self.stmt_indent = indent;
        }
        if case_label && in_switch {
            if let Some(s) = self.stack.last_mut() {
                s.in_case = true;
            }
        }

        // Text, feeding the scopes as each bracket goes by.
        let mut text = String::new();
        for k in first..=last {
            if k > first {
                text.push_str(&gap_text(toks, ctx, k));
            }
            let width_before = indent + text_width(&text, iw);
            let raw = &toks[k];
            if raw.kind == Kind::BlockComment
                && raw.text.contains('\n')
                && (style.align_block_comments || style.wrap_javadoc)
            {
                let orig = (k == first && raw.gap.contains('\n'))
                    .then(|| text_width(raw.gap.rsplit('\n').next().unwrap_or(""), iw));
                text.push_str(&super::comments::realign(&raw.text, width_before, orig, style));
            } else {
                text.push_str(&raw.text);
            }
            let col_after = indent + text_width(&text, iw);
            self.feed(toks, ctx, k, col_after.max(width_before));
        }

        if !comment_only {
            let last_code = (first..=last).rev().find(|&k| !toks[k].is_comment()).unwrap_or(last);
            self.prev_last = Some(last_code);
            self.prev_annotations_only = only_annotations(toks, ctx.info, first, last_code);
            self.pending_body = self.header_expects_body(toks, ctx, last_code);
        }
        LineOut { indent, width: indent + text_width(&text, iw), text }
    }

    fn header_expects_body(&self, toks: &[Tok], ctx: &Ctx, last: usize) -> bool {
        let t = &toks[last];
        if t.is("else") {
            return true;
        }
        if t.is(")") {
            let open = ctx.info[last].matching;
            if open != NO_MATCH {
                if let Some(p) = prev_sig(toks, open) {
                    return toks[p].is_word() && matches!(toks[p].text.as_str(), "if" | "for" | "while");
                }
            }
        }
        false
    }

    fn feed(&mut self, toks: &[Tok], ctx: &Ctx, k: usize, col_after: usize) {
        let t = &toks[k];
        if t.kind != Kind::Punct {
            return;
        }
        match t.text.as_str() {
            "(" | "[" => self.stack.push(Scope { kind: ScopeKind::Paren, anchor: self.cur_indent, align: col_after, in_case: false }),
            "{" => {
                let kind = ctx.info[k].brace.unwrap_or(BraceKind::Block);
                let anchor = match kind {
                    BraceKind::Lambda | BraceKind::Anon | BraceKind::Array => self.cur_indent,
                    _ => self.stmt_indent,
                };
                self.stack.push(Scope { kind: ScopeKind::Brace(kind), anchor, align: col_after, in_case: false });
            }
            ")" | "]" | "}" => {
                self.stack.pop();
            }
            _ => {}
        }
    }
}

/// The whitespace between token `k` and the one before it on the same line.
pub(crate) fn gap_text(toks: &[Tok], ctx: &Ctx, k: usize) -> String {
    let t = &toks[k];
    let prev = &toks[k - 1];
    let mut gap = match ctx.gaps[k] {
        Gap::One => " ".to_string(),
        Gap::Zero => String::new(),
        Gap::Keep => {
            if t.gap.contains('\n') || t.gap.is_empty() && t.ws_before { " ".to_string() } else { t.gap.clone() }
        }
    };
    // Glued tokens must still read as the same tokens.
    if gap.is_empty() && needs_space(prev, t) {
        gap.push(' ');
    }
    gap
}

fn needs_space(a: &Tok, b: &Tok) -> bool {
    let wordish = |c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '"' || c == '\'';
    let (Some(x), Some(y)) = (a.text.chars().last(), b.text.chars().next()) else { return false };
    if wordish(x) && wordish(y) {
        return true;
    }
    if a.kind == Kind::Punct && b.kind == Kind::Punct {
        let glued = format!("{}{}", a.text, b.text);
        let relexed = super::lex::lex(&glued);
        return relexed.len() != 2 || relexed[0].text != a.text;
    }
    // A comment glued to a token would swallow or be swallowed by it.
    a.kind == Kind::LineComment || (a.kind == Kind::Punct && a.text == "/" && b.text.starts_with('*')) || (b.kind == Kind::Punct && b.text == "/" && a.text.ends_with('/'))
}

/// `cols` columns of indentation, as tabs (plus the remainder in spaces) or spaces.
pub(crate) fn indent_string(cols: usize, style: &FormatStyle) -> String {
    if style.use_tabs {
        let iw = style.indent_width.max(1);
        format!("{}{}", "\t".repeat(cols / iw), " ".repeat(cols % iw))
    } else {
        " ".repeat(cols)
    }
}

/// Group `first..=last` token ranges by line-break flags: `(first, last)` of every line in `lo..=hi`.
pub(crate) fn lines_of(toks: &[Tok], lo: usize, hi: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut first = lo;
    for i in lo + 1..=hi {
        if toks[i].nl_before > 0 || toks[i - 1].kind == Kind::LineComment {
            out.push((first, i - 1));
            first = i;
        }
    }
    out.push((first, hi));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::classify::classify;
    use crate::format::lex::lex;
    use crate::format::spacing::gaps;

    fn indents(src: &str, style: &FormatStyle) -> Vec<usize> {
        let toks = lex(src);
        let info = classify(&toks);
        let g = gaps(&toks, &info, style);
        let ctx = Ctx { info: &info, gaps: &g, style };
        let mut m = Machine::new(0);
        lines_of(&toks, 0, toks.len() - 1)
            .into_iter()
            .map(|(f, l)| m.layout_line(&toks, &ctx, f, l).indent)
            .collect()
    }

    #[test]
    fn braces_indent_by_one_level_and_closers_come_back() {
        let st = FormatStyle::default();
        assert_eq!(indents("class A {\nvoid m() {\nint x;\n}\n}\n", &st), [0, 4, 8, 4, 0]);
    }

    #[test]
    fn wrapped_arguments_and_binary_continuations_use_the_continuation_indent() {
        let st = FormatStyle::default();
        assert_eq!(indents("void m() {\nfoo(a,\nb);\nint x = a\n+ b;\n}\n", &st), [0, 4, 12, 4, 12, 0]);
    }

    #[test]
    fn a_case_label_sits_inside_the_switch_and_its_body_one_further() {
        let st = FormatStyle::default();
        let got = indents("switch (x) {\ncase 1:\nf();\nbreak;\n}\n", &st);
        assert_eq!(got, [0, 4, 8, 8, 0]);
        let flat = FormatStyle { indent_case_body: false, ..FormatStyle::default() };
        assert_eq!(indents("switch (x) {\ncase 1:\nf();\n}\n", &flat), [0, 4, 4, 0]);
    }

    #[test]
    fn an_unbraced_body_and_an_allman_brace() {
        let st = FormatStyle::default();
        assert_eq!(indents("if (a)\nfoo();\nbar();\n", &st), [0, 4, 0]);
        assert_eq!(indents("void m()\n{\nx();\n}\n", &st), [0, 0, 4, 0]);
    }

    #[test]
    fn a_lambda_body_is_indented_from_the_line_that_opened_it() {
        let st = FormatStyle::default();
        assert_eq!(indents("void m() {\nlist.forEach(x -> {\nf(x);\n});\n}\n", &st), [0, 4, 8, 4, 0]);
    }
}
